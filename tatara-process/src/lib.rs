//! Process CRD — the K8s-as-Unix-processes wire format.
//!
//! A `Process` is one element of the tatara convergence lattice.
//! Clusters, HelmReleases, migrations, tests — all are Processes.
//! The reconciliation loop *is* Unix: fork → exec → wait → exit → reap.

pub mod allocation;
pub mod attestation;
pub mod boundary;
pub mod classification;
pub mod compliance;
pub mod crd;
pub mod encapsulates;
pub mod env;
pub mod ephemeral;
pub mod export;
pub mod hostname;
pub mod identity;
pub mod intent;
pub mod lifetime;
pub mod lifetime_clock;
pub mod matrix;
pub mod phase;
pub mod pool;
pub mod receipt;
pub mod routing;
pub mod signal;
pub mod spec;
pub mod status;
pub mod table;
pub mod tagged_union;

pub mod prelude {
    pub use crate::allocation::{
        AllocationCondition, AllocationPhase, AllocationSpec, AllocationStatus,
        EphemeralAllocation, Requestor,
    };
    pub use crate::attestation::ProcessAttestation;
    pub use crate::boundary::{Boundary, Condition, ConditionKind, UnknownConditionKind};
    pub use crate::classification::{
        Arity, CalmClassification, Classification, ConvergencePointType, DataClassification,
        Horizon, HorizonKind, OptimizationDirection, SubstrateType, UnknownCalmClassification,
        UnknownConvergencePointType, UnknownDataClassification, UnknownHorizonKind,
        UnknownOptimizationDirection, UnknownSubstrateType,
    };
    pub use crate::compliance::{
        ComplianceBinding, ComplianceSpec, UnknownVerificationPhase, VerificationPhase,
    };
    pub use crate::crd::{Process, ProcessSpec, ProcessStatus};
    pub use crate::encapsulates::{
        BareWorkload, EncapsulatesSpec, EncapsulationKind, EncapsulationKindError,
        EncapsulationKindVariant, EncapsulationMode, EncapsulationTarget, ExistingHelmRelease,
        ExistingKustomization, UnknownEncapsulationMode, UnknownEncapsulationTarget,
    };
    pub use crate::ephemeral::{compile_ephemeral_source, EphemeralSpec};
    pub use crate::export::{
        ArtifactError, ArtifactKind, ArtifactSource, ArtifactVariant, ChannelError, ChannelKind,
        ChannelVariant, ExportSpec, ExportTrigger, HttpEventChannel, NatsSubjectChannel,
        ProcessSnapshotSource, ReceiptsSource, ReportFormat, ReportPayloadShape, RunMarkerSource,
        StdoutChannel, TestReportSource, UnknownArtifactKind, UnknownChannelKind,
        UnknownExportTrigger, UnknownReportFormat, VectorChannel, DEFAULT_NATS_URL,
        DEFAULT_VECTOR_INGEST,
    };
    pub use crate::hostname::{
        ephemeral_id_from_spec, fmt_fqdn, fmt_fqdn_stable, resolve_ephemeral_id, HostnameError,
        EPHEMERAL_ID_HASH_LEN,
    };
    pub use crate::identity::{content_hash, derive_identity, format_process_address, Identity};
    pub use crate::intent::{
        AplicacaoIntent, ContainerIntent, FluxIntent, GuestIntent, HelmLifecyclePolicy,
        HelmRemediationPolicy, Intent, IntentError, IntentKind, IntentVariant, LispIntent,
        NixIntent, UnknownWorkloadKind, WorkloadKind, FLUX_HELM_DEFAULT_INTERVAL,
        HELM_LIFECYCLE_DEFAULT_RETRIES, HELM_LIFECYCLE_DEFAULT_TIMEOUT,
    };
    pub use crate::lifetime::{
        EphemeralLifetime, Lifetime, LifetimeError, LifetimeKind, LifetimeVariant,
        PermanentLifetime, TeardownPolicy, UnknownTeardownPolicy,
    };
    pub use crate::lifetime_clock::{
        evaluate as lifetime_clock_evaluate, AutoTerminate, AutoTerminateKind, TerminateReason,
        TerminateReasonKind, UnknownAutoTerminateKind, UnknownTerminateReasonKind,
    };
    pub use crate::matrix::{
        compile_env_matrix_source, EnvMatrixSpec, MatrixAxis, MatrixBudget, NamedEphemeral,
        SelectStrategy, SelectStrategyKind, UnknownSelectStrategyKind,
    };
    pub use crate::phase::{ProcessPhase, UnknownPhase};
    pub use crate::pool::{
        AllocationRef, EphemeralPool, MatchKey, MemberState, PoolCondition, PoolMember, PoolPhase,
        PoolSelector, PoolSpec, PoolStatus, ReplacementPolicy, ReturnPolicy, UnknownMemberState,
        UnknownPoolPhase, UnknownReplacementPolicy,
    };
    pub use crate::receipt::{
        default_receipt_config_map_name, ReceiptEnvelope, ReceiptError, ReceiptKind,
        RECEIPT_CM_SUFFIX, RECEIPT_VERSION,
    };
    pub use crate::routing::{RoutingBackend, RoutingHostname, RoutingSpec};
    pub use crate::signal::{ProcessSignal, SighupStrategy, UnknownSighupStrategy};
    pub use crate::spec::{
        DependsOn, IdentitySpec, MustReachPhase, SignalPolicy, UnknownMustReachPhase,
    };
    pub use crate::status::{
        BoundaryStatus, CheckedCondition, ComplianceStatus, FluxResourceRef, ProcessCondition,
        RenderedResourceCoords,
    };
    pub use crate::table::{
        ClaimRecord, ProcessEntry, ProcessTable, ProcessTableSpec, ProcessTableStatus,
    };
}

/// CRD API group for every tatara CRD.
pub const GROUP: &str = "tatara.pleme.io";
/// CRD version for this module.
pub const VERSION: &str = "v1alpha1";
/// Kind spelling of the tatara Process CRD as it appears in a K8s
/// [`OwnerReference.kind`][ownref] field. Peer to [`GROUP`] +
/// [`VERSION`] — centralizes the ONE literal every SSA-time
/// re-injection helper pre-lift restated by hand across
/// `tatara-reconciler` (`render.rs`, `edges.rs`, `ssapply.rs`).
///
/// [ownref]: https://kubernetes.io/docs/concepts/overview/working-with-objects/owners-dependents/
pub const PROCESS_KIND: &str = "Process";

/// Canonical `<GROUP>/<VERSION>` as an owned `String` — the ONE
/// K8s `apiVersion` shape every tatara CRD stamps. Composed from
/// [`GROUP`] + [`VERSION`] so a bump of either constant lands here
/// exactly once; pre-lift, two `tatara-reconciler` sites hand-wrote
/// `format!("{}/{}", tatara_process::GROUP, tatara_process::VERSION)`
/// while a third inlined the literal `"tatara.pleme.io/v1alpha1"`,
/// opening a silent drift path if `VERSION` ever advances past
/// `v1alpha1`.
pub fn api_version() -> String {
    format!("{GROUP}/{VERSION}")
}

/// Build a Kubernetes [`OwnerReference`][ownref] JSON blob pointing
/// at a Process (`kind = `[`PROCESS_KIND`], `apiVersion = `
/// [`api_version`]) with `controller: true` +
/// `blockOwnerDeletion: true` — the exact 6-slot shape every SSA
/// re-injection site pre-lift restated three times across
/// `tatara-reconciler` (`render.rs::owner_refs` for export-Job
/// owners, `edges.rs::build_owner_refs` for Ingress + DNSEndpoint
/// owners, `ssapply.rs::build_owner_reference` for the injected
/// owner-ref stamped on every applied `DynamicObject`). Callers
/// with a live `Process` value read `metadata.{name,uid}` and pass
/// them through as `&str`.
///
/// The 6-slot shape is fixed (`controller` + `blockOwnerDeletion`
/// both `true`); a Process-owned resource that wants a non-
/// controller reference doesn't belong on this owner and can build
/// its own `json!` inline — this primitive is the composer for the
/// canonical "Process controls this resource, cascade-delete on
/// GC" shape, not a general OwnerReference builder.
///
/// [ownref]: https://kubernetes.io/docs/concepts/overview/working-with-objects/owners-dependents/
pub fn owner_reference_json(name: &str, uid: &str) -> serde_json::Value {
    serde_json::json!({
        "apiVersion": api_version(),
        "kind": PROCESS_KIND,
        "name": name,
        "uid": uid,
        "controller": true,
        "blockOwnerDeletion": true,
    })
}

/// Annotation keys the reconciler reads/writes on owned FluxCD resources.
pub mod annotations {
    pub const MANAGED_BY: &str = "tatara.pleme.io/managed-by";
    pub const PROCESS: &str = "tatara.pleme.io/process";
    pub const PID: &str = "tatara.pleme.io/pid";
    pub const CONTENT_HASH: &str = "tatara.pleme.io/content-hash";
    pub const ATTESTATION_ROOT: &str = "tatara.pleme.io/attestation-root";
    pub const GENERATION: &str = "tatara.pleme.io/generation";
    pub const SIGNAL: &str = "tatara.pleme.io/signal";
    /// Stamped by the reconciler when transitioning into `Releasing`
    /// — records which terminal-reached gate the Process came from
    /// (`Attested` or `Failed`) so `handle_releasing` can pick the
    /// matching `ExportTrigger` set + the correct post-Releasing
    /// destination (`Exiting` from Attested, `Zombie` from Failed).
    pub const RELEASED_FROM: &str = "tatara.pleme.io/released-from";
    /// Labels the export-worker Jobs the reconciler emits during
    /// `Releasing`. Selector: `tatara.pleme.io/role=export`.
    pub const ROLE: &str = "tatara.pleme.io/role";
    /// Index of an export inside `lifetime.ephemeral.exports`.
    /// Stamped on the corresponding tatara-export-worker Job + its
    /// receipt ConfigMap so the reconciler can correlate them
    /// without re-parsing the spec JSON.
    pub const EXPORT_INDEX: &str = "tatara.pleme.io/export-index";
}

/// Standard finalizer for the Process reconciler.
pub const PROCESS_FINALIZER: &str = "tatara.pleme.io/process-finalizer";

/// Shared schemars helpers — emit OpenAPI schemas Kubernetes accepts.
/// Free-form `serde_json::Value` fields default to an *empty* schema
/// in schemars, which the K8s API server rejects with "type: Required
/// value: must not be empty for specified object fields". The typed
/// workaround is to emit `{type: object, x-kubernetes-preserve-unknown-
/// fields: true}` — same shape kube-rs's own helpers produce.
pub mod schema_helpers {
    use schemars::{gen::SchemaGenerator, schema::Schema};
    /// Schema for a free-form JSON object field. Apply via
    /// `#[schemars(schema_with = "tatara_process::schema_helpers::preserve_unknown_object")]`
    /// on any `serde_json::Value` / `BTreeMap<String, serde_json::Value>`
    /// field exposed through a CRD.
    pub fn preserve_unknown_object(_g: &mut SchemaGenerator) -> Schema {
        serde_json::from_value(serde_json::json!({
            "type": "object",
            "x-kubernetes-preserve-unknown-fields": true
        }))
        .expect("static JSON literal parses as Schema")
    }
}

#[cfg(test)]
mod owner_reference_tests {
    //! Pin the `owner_reference_json` composer at fail-before-pass-
    //! after granularity. Every shape a pre-lift caller hand-authored
    //! is re-asserted here so a regression that inlined any of the
    //! six slots at a call site (breaking the primitive's role as
    //! the ONE source of truth) fails HERE at the composer's shipped-
    //! shape pin rather than as silent drift between the pre-lift
    //! `render.rs` / `edges.rs` / `ssapply.rs` sites (which pre-lift
    //! already carried TWO different `apiVersion` spellings — a
    //! composed `format!("{}/{}", GROUP, VERSION)` at two sites and
    //! the frozen literal `"tatara.pleme.io/v1alpha1"` at the third).
    use super::{api_version, owner_reference_json, GROUP, PROCESS_KIND, VERSION};
    use serde_json::json;

    #[test]
    fn api_version_composes_group_and_version() {
        // Any bump of GROUP or VERSION lands at ONE composer.
        assert_eq!(api_version(), format!("{GROUP}/{VERSION}"));
    }

    #[test]
    fn api_version_byte_matches_wire_form_pre_lift() {
        // Byte-identity pin: the frozen wire-form literal
        // `"tatara.pleme.io/v1alpha1"` that `ssapply.rs::
        // build_owner_reference` hand-wrote pre-lift must equal the
        // composed shape now sourced through the ONE owner. A
        // future VERSION bump that missed this test would land as
        // an operator-visible reference-mismatch after apply.
        assert_eq!(api_version(), "tatara.pleme.io/v1alpha1");
    }

    #[test]
    fn process_kind_is_process_literal() {
        // Symbol-vs-string pin: any consumer that hand-wrote `"Process"`
        // pre-lift routes through this const post-lift.
        assert_eq!(PROCESS_KIND, "Process");
    }

    #[test]
    fn owner_reference_json_has_all_six_slots_present() {
        let v = owner_reference_json("my-process", "abc-uid");
        let obj = v.as_object().expect("owner reference is a JSON object");
        for k in [
            "apiVersion",
            "kind",
            "name",
            "uid",
            "controller",
            "blockOwnerDeletion",
        ] {
            assert!(obj.contains_key(k), "missing owner-reference slot: {k}");
        }
        assert_eq!(obj.len(), 6, "owner reference must have exactly 6 slots");
    }

    #[test]
    fn owner_reference_json_apiversion_routes_through_api_version_owner() {
        let v = owner_reference_json("x", "y");
        assert_eq!(v["apiVersion"], api_version());
    }

    #[test]
    fn owner_reference_json_kind_routes_through_process_kind_const() {
        let v = owner_reference_json("x", "y");
        assert_eq!(v["kind"], PROCESS_KIND);
    }

    #[test]
    fn owner_reference_json_stamps_supplied_name_and_uid() {
        let v = owner_reference_json("some-name", "some-uid");
        assert_eq!(v["name"], "some-name");
        assert_eq!(v["uid"], "some-uid");
    }

    #[test]
    fn owner_reference_json_controller_and_block_owner_deletion_are_true() {
        // These are structural — a Process-owned resource always
        // has a controlling reference that cascade-deletes with
        // the owner. A regression that flipped either boolean
        // would silently detach every emitted resource.
        let v = owner_reference_json("x", "y");
        assert_eq!(v["controller"], true);
        assert_eq!(v["blockOwnerDeletion"], true);
    }

    #[test]
    fn owner_reference_json_matches_hand_authored_shape_pre_lift() {
        // Byte-shape pin against the exact `json!({…})` incantation
        // every pre-lift call site restated. A regression that
        // reordered a slot, dropped one, or added a seventh here
        // surfaces at THIS pin rather than as a subtle SSA-apply
        // failure downstream when the K8s API server rejects the
        // OwnerReference on schema mismatch.
        let via_owner = owner_reference_json("p", "u");
        let hand_authored = json!({
            "apiVersion": "tatara.pleme.io/v1alpha1",
            "kind": "Process",
            "name": "p",
            "uid": "u",
            "controller": true,
            "blockOwnerDeletion": true,
        });
        assert_eq!(via_owner, hand_authored);
    }

    #[test]
    fn owner_reference_json_preserves_empty_name_and_uid_bytewise() {
        // The primitive does not guard against empty inputs — its
        // callers pre-lift did the empty-check upstream
        // (`render.rs` guards `!uid.is_empty()`,
        // `edges.rs::build_owner_refs` returns `vec![]` on empty
        // uid, `ssapply.rs::build_owner_reference` unwraps a
        // required `metadata.uid` via anyhow). The primitive owns
        // shape composition, not admission control; a downstream
        // rename that wants strict input validation lands as a
        // peer, not a change to the composer's contract.
        let v = owner_reference_json("", "");
        assert_eq!(v["name"], "");
        assert_eq!(v["uid"], "");
    }
}

// ── Lisp → ProcessSpec compile bridge ──────────────────────────────────
//
// `(defpoint NAME :k v …)` compiles to a `NamedDefinition<ProcessSpec>`.
// The derive on ProcessSpec handles every field via the serde Deserialize
// fallthrough — no hand-rolled keyword parsing needed.

/// A named ProcessSpec as produced by `compile_source`.
pub type Definition = tatara_lisp::NamedDefinition<crate::crd::ProcessSpec>;

/// Compile a Lisp source string into a list of named ProcessSpecs.
/// Each top-level `(defpoint NAME …)` form becomes one `Definition`.
pub fn compile_source(src: &str) -> tatara_lisp::Result<Vec<Definition>> {
    tatara_lisp::compile_named::<crate::crd::ProcessSpec>(src)
}

/// Register every domain owned by this crate with the global Lisp
/// dispatcher. Call once per binary, typically near the top of `main`.
/// After this call, `tatara_lisp::domain::lookup("defpoint")` and
/// `lookup("defephemeral")` both resolve to the right typed compiler.
///
/// Idempotent — registering the same type twice is a no-op.
pub fn register_all() {
    tatara_lisp::domain::register::<crate::crd::ProcessSpec>();
    tatara_lisp::domain::register::<crate::ephemeral::EphemeralSpec>();
}

#[cfg(test)]
mod compile_tests {
    use super::compile_source;
    use crate::classification::{ConvergencePointType, SubstrateType};
    use crate::compliance::VerificationPhase;
    use crate::spec::MustReachPhase;

    /// The full derive-powered pipeline — no hand-rolled parsing anywhere.
    /// Every field travels: Lisp → Sexp → serde_json → typed ProcessSpec.
    #[test]
    fn full_processspec_round_trip_via_derive() {
        let src = r#"
            (defpoint observability-stack
              :identity       (:parent "seph.1")
              :classification (:point-type Gate
                               :substrate Observability
                               :horizon (:kind Bounded)
                               :calm Monotone
                               :data-classification Internal)
              :intent         (:nix (:flake-ref "github:pleme-io/k8s"
                                     :attribute "observability"
                                     :attic-cache "main"))
              :boundary       (:postconditions
                                 ((:kind KustomizationHealthy
                                   :params (:name "observability-stack"
                                            :namespace "flux-system"))
                                  (:kind PromQL
                                   :params (:query "up == 1")))
                               :timeout "15m")
              :compliance     (:baseline "fedramp-moderate"
                               :bindings ((:framework "nist-800-53"
                                           :control-id "SC-7"
                                           :phase AtBoundary)))
              :depends-on     ((:name "secret-injection" :must-reach Attested))
              :signals        (:sigterm-grace-seconds 480
                               :sighup-strategy Reconverge))
        "#;
        let defs = compile_source(src).expect("compile");
        assert_eq!(defs.len(), 1);
        let d = &defs[0];
        assert_eq!(d.name, "observability-stack");

        // identity
        assert_eq!(d.spec.identity.parent.as_deref(), Some("seph.1"));

        // classification (enums deserialized via symbol → string)
        assert_eq!(d.spec.classification.point_type, ConvergencePointType::Gate);
        assert_eq!(
            d.spec.classification.substrate,
            SubstrateType::Observability
        );

        // intent (tagged-union with one of four options)
        let nix = d.spec.intent.nix.as_ref().expect("nix intent");
        assert_eq!(nix.flake_ref, "github:pleme-io/k8s");
        assert_eq!(nix.attribute, "observability");
        assert_eq!(nix.attic_cache.as_deref(), Some("main"));

        // boundary (Vec<nested struct with params object>)
        assert_eq!(d.spec.boundary.postconditions.len(), 2);
        assert_eq!(d.spec.boundary.timeout.as_deref(), Some("15m"));

        // compliance (Vec<binding with enum phase>)
        assert_eq!(
            d.spec.compliance.baseline.as_deref(),
            Some("fedramp-moderate")
        );
        assert_eq!(d.spec.compliance.bindings.len(), 1);
        assert_eq!(
            d.spec.compliance.bindings[0].phase,
            VerificationPhase::AtBoundary
        );

        // depends_on (Vec<struct with enum>)
        assert_eq!(d.spec.depends_on.len(), 1);
        assert_eq!(d.spec.depends_on[0].must_reach, MustReachPhase::Attested);

        // signals (numeric + enum defaults)
        assert_eq!(d.spec.signals.sigterm_grace_seconds, 480);
    }

    #[test]
    fn missing_required_field_errors() {
        // `:classification` has no #[serde(default)] — omit it and compile must fail.
        let src = r#"(defpoint x :intent (:nix (:flake-ref "f" :attribute "a")))"#;
        assert!(compile_source(src).is_err());
    }

    #[test]
    fn serde_default_fields_are_optional() {
        // Omit every #[serde(default)] field — compile must succeed because
        // the derive honors serde defaults.
        let src = r#"
            (defpoint x
              :classification (:point-type Transform :substrate Compute)
              :intent (:flux (:git-repository "g" :path ".")))
        "#;
        let defs = compile_source(src).expect("compile");
        assert_eq!(defs.len(), 1);
        let d = &defs[0];
        assert!(d.spec.depends_on.is_empty());
        assert!(d.spec.boundary.postconditions.is_empty());
        assert!(d.spec.compliance.bindings.is_empty());
        assert!(!d.spec.suspended);
        // Lifetime defaults to Permanent (no variant set, resolver still works).
        assert!(d.spec.lifetime.is_default());
        assert!(!d.spec.lifetime.is_ephemeral());
    }

    /// Registering all process-owned domains is idempotent and resolves
    /// both `defpoint` (ProcessSpec) and `defephemeral` (EphemeralSpec).
    #[test]
    fn register_all_resolves_defpoint_and_defephemeral() {
        use tatara_lisp::domain::lookup;
        super::register_all();
        super::register_all(); // idempotent
        assert!(lookup("defpoint").is_some(), "defpoint must resolve");
        assert!(
            lookup("defephemeral").is_some(),
            "defephemeral must resolve"
        );
    }

    /// End-to-end: a `(defpoint …)` form may carry the full ephemeral
    /// shape directly — `:intent (:aplicacao …)` + `:lifetime (:ephemeral …)`.
    /// This is what the `(defephemeral …)` sugar lowers to via `From`.
    #[test]
    fn defpoint_with_aplicacao_intent_and_ephemeral_lifetime() {
        use crate::intent::IntentVariant;
        use crate::lifetime::{LifetimeVariant, TeardownPolicy};
        let src = r#"
            (defpoint closed-loop-attest
              :classification (:point-type Gate :substrate Compute)
              :intent (:aplicacao
                        (:chart-ref "oci://ghcr.io/pleme-io/charts/lareira-demo-app"
                         :version "0.5.5"
                         :profile "all-in-one"
                         :values-overlay (:cluster (:name "ephemeral-test-01"))
                         :target-namespace "demo-test"))
              :boundary (:postconditions
                          ((:kind HelmReleaseReleased
                            :params (:name "demo-app-consolidated"
                                     :namespace "demo-test"))
                           (:kind ClosedLoopAuth
                            :params (:issuer (:service "demo-app-issuer" :port 8080)
                                     :consumer (:service "demo-app-gateway" :port 8000)
                                     :probeImage "ghcr.io/pleme-io/closed-loop-probe:0.1.0"))))
              :lifetime (:ephemeral (:ttl "1h"
                                     :teardown-policy OnAttested
                                     :max-concurrent 1)))
        "#;
        let defs = compile_source(src).expect("compile");
        assert_eq!(defs.len(), 1);
        let d = &defs[0];

        // Aplicacao intent landed.
        match d.spec.intent.variant().unwrap() {
            IntentVariant::Aplicacao(a) => {
                assert_eq!(a.profile, "all-in-one");
                assert_eq!(a.version, "0.5.5");
                assert_eq!(a.target_namespace.as_deref(), Some("demo-test"));
                assert_eq!(a.values_overlay["cluster"]["name"], "ephemeral-test-01");
            }
            other => panic!("expected Aplicacao, got {other:?}"),
        }

        // Ephemeral lifetime landed with the right teardown policy.
        match d.spec.lifetime.variant().unwrap() {
            LifetimeVariant::Ephemeral(e) => {
                assert_eq!(e.ttl, "1h");
                assert_eq!(e.teardown_policy, TeardownPolicy::OnAttested);
                assert_eq!(e.max_concurrent, 1);
            }
            other => panic!("expected ephemeral, got {other:?}"),
        }

        // Two typed postconditions including ClosedLoopAuth.
        assert_eq!(d.spec.boundary.postconditions.len(), 2);
        assert_eq!(
            d.spec.boundary.postconditions[1].kind,
            crate::boundary::ConditionKind::ClosedLoopAuth
        );
    }
}
