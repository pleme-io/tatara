//! Process CRD — the K8s-as-Unix-processes wire format.
//!
//! A `Process` is one element of the tatara convergence lattice.
//! Clusters, HelmReleases, migrations, tests — all are Processes.
//! The reconciliation loop *is* Unix: fork → exec → wait → exit → reap.

pub mod attestation;
pub mod boundary;
pub mod classification;
pub mod compliance;
pub mod crd;
pub mod identity;
pub mod intent;
pub mod phase;
pub mod signal;
pub mod spec;
pub mod status;
pub mod table;

pub mod prelude {
    pub use crate::attestation::ProcessAttestation;
    pub use crate::boundary::{Boundary, Condition, ConditionKind};
    pub use crate::classification::{
        CalmClassification, Classification, ConvergencePointType, DataClassification, Horizon,
        HorizonKind, OptimizationDirection, SubstrateType,
    };
    pub use crate::compliance::{ComplianceBinding, ComplianceSpec, VerificationPhase};
    pub use crate::crd::{Process, ProcessSpec, ProcessStatus};
    pub use crate::identity::{content_hash, derive_identity, format_process_address, Identity};
    pub use crate::intent::{
        ContainerIntent, FluxIntent, Intent, LispIntent, NixIntent, WorkloadKind,
    };
    pub use crate::phase::ProcessPhase;
    pub use crate::signal::{ProcessSignal, SighupStrategy};
    pub use crate::spec::{DependsOn, IdentitySpec, MustReachPhase, SignalPolicy};
    pub use crate::status::{
        BoundaryStatus, CheckedCondition, ComplianceStatus, FluxResourceRef, ProcessCondition,
    };
    pub use crate::table::{ProcessEntry, ProcessTable, ProcessTableSpec, ProcessTableStatus};
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
}

/// Standard finalizer for the Process reconciler.
pub const PROCESS_FINALIZER: &str = "tatara.pleme.io/process-finalizer";

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
    }
}
