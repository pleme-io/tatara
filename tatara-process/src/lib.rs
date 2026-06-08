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
        CalmClassification, Classification, ConvergencePointType, DataClassification, Horizon,
        HorizonKind, OptimizationDirection, SubstrateType,
    };
    pub use crate::compliance::{ComplianceBinding, ComplianceSpec, VerificationPhase};
    pub use crate::crd::{Process, ProcessSpec, ProcessStatus};
    pub use crate::encapsulates::{
        BareWorkload, EncapsulatesSpec, EncapsulationKind, EncapsulationKindError,
        EncapsulationKindVariant, EncapsulationMode, ExistingHelmRelease, ExistingKustomization,
    };
    pub use crate::ephemeral::{compile_ephemeral_source, EphemeralSpec};
    pub use crate::export::{
        ArtifactError, ArtifactSource, ArtifactVariant, ChannelError, ChannelVariant, ExportSpec,
        ExportTrigger, HttpEventChannel, NatsSubjectChannel, ProcessSnapshotSource, ReceiptsSource,
        ReportFormat, RunMarkerSource, StdoutChannel, TestReportSource, VectorChannel,
        DEFAULT_NATS_URL, DEFAULT_VECTOR_INGEST,
    };
    pub use crate::hostname::{
        ephemeral_id_from_spec, fmt_fqdn, fmt_fqdn_stable, resolve_ephemeral_id, HostnameError,
        EPHEMERAL_ID_HASH_LEN,
    };
    pub use crate::identity::{content_hash, derive_identity, format_process_address, Identity};
    pub use crate::intent::{
        AplicacaoIntent, ContainerIntent, FluxIntent, GuestIntent, Intent, IntentError, IntentKind,
        IntentVariant, LispIntent, NixIntent, WorkloadKind,
    };
    pub use crate::lifetime::{
        EphemeralLifetime, Lifetime, LifetimeError, LifetimeKind, LifetimeVariant,
        PermanentLifetime, TeardownPolicy,
    };
    pub use crate::lifetime_clock::{evaluate as lifetime_clock_evaluate, AutoTerminate};
    pub use crate::matrix::{
        compile_env_matrix_source, EnvMatrixSpec, MatrixAxis, MatrixBudget, NamedEphemeral,
        SelectStrategy,
    };
    pub use crate::phase::{ProcessPhase, UnknownPhase};
    pub use crate::pool::{
        AllocationRef, EphemeralPool, MatchKey, MemberState, PoolCondition, PoolMember, PoolPhase,
        PoolSelector, PoolSpec, PoolStatus, ReturnPolicy,
    };
    pub use crate::receipt::{ReceiptEnvelope, ReceiptError, RECEIPT_VERSION};
    pub use crate::routing::{RoutingBackend, RoutingHostname, RoutingSpec};
    pub use crate::signal::{ProcessSignal, SighupStrategy};
    pub use crate::spec::{DependsOn, IdentitySpec, MustReachPhase, SignalPolicy};
    pub use crate::status::{
        BoundaryStatus, CheckedCondition, ComplianceStatus, FluxResourceRef, ProcessCondition,
    };
    pub use crate::table::{
        ClaimRecord, ProcessEntry, ProcessTable, ProcessTableSpec, ProcessTableStatus,
    };
}

/// CRD API group for every tatara CRD.
pub const GROUP: &str = "tatara.pleme.io";
/// CRD version for this module.
pub const VERSION: &str = "v1alpha1";

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
              :depends-on     ((:name "akeyless" :must-reach Attested))
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
            (defpoint akeyless-closed-loop-attest
              :classification (:point-type Gate :substrate Compute)
              :intent (:aplicacao
                        (:chart-ref "oci://ghcr.io/pleme-io/charts/lareira-akeyless-deployment"
                         :version "0.5.5"
                         :profile "gateway-with-internal-saas"
                         :values-overlay (:cluster (:name "ephemeral-test-01"))
                         :target-namespace "akeyless-test"))
              :boundary (:postconditions
                          ((:kind HelmReleaseReleased
                            :params (:name "akeyless-saas-consolidated"
                                     :namespace "akeyless-test"))
                           (:kind ClosedLoopAuth
                            :params (:issuer (:service "akeyless-saas-akeyless-gator" :port 8080)
                                     :consumer (:service "akeyless-saas-akeyless-gateway" :port 8000)
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
                assert_eq!(a.profile, "gateway-with-internal-saas");
                assert_eq!(a.version, "0.5.5");
                assert_eq!(a.target_namespace.as_deref(), Some("akeyless-test"));
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
