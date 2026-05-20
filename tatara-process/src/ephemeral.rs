//! `EphemeralSpec` — the operator-facing typed surface for ephemeral
//! Aplicacao installations.
//!
//! `EphemeralSpec` is *sugar* on top of `ProcessSpec`. The compounding move
//! is to keep one wire format (`Process`, the Unix-process CRD) and let
//! ephemeral envs be a Process with `:intent (:aplicacao …)` +
//! `:lifetime (:ephemeral …)`. This struct gives that combination a
//! dedicated `(defephemeral …)` keyword and a typed `From` bridge so
//! authoring stays first-class without forking the CRD.
//!
//! Lisp authoring:
//! ```lisp
//! (defephemeral akeyless-closed-loop-attest
//!   :aplicacao  (:chart-ref "oci://ghcr.io/pleme-io/charts/lareira-akeyless-deployment"
//!                :version "0.5.5"
//!                :profile "gateway-with-internal-saas"
//!                :values-overlay (:cluster (:name "ephemeral-test-01")
//!                                 :persistence false))
//!   :ttl        "1h"
//!   :teardown   OnAttested
//!   :postconditions
//!     ((:kind HelmReleaseReleased
//!       :params (:name "akeyless-saas-consolidated"
//!                :namespace "akeyless-test"))
//!      (:kind ClosedLoopAuth
//!       :params (:issuer (:service "akeyless-saas-akeyless-gator" :port 8080)
//!                :consumer (:service "akeyless-saas-akeyless-gateway" :port 8000)
//!                :probeImage "ghcr.io/pleme-io/closed-loop-probe:0.1.0"))))
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

use crate::boundary::{Boundary, Condition};
use crate::classification::{
    Classification, ConvergencePointType, DataClassification, Horizon, SubstrateType,
};
use crate::crd::ProcessSpec;
use crate::export::ExportSpec;
use crate::intent::{AplicacaoIntent, Intent};
use crate::lifetime::{EphemeralLifetime, Lifetime, TeardownPolicy};

/// `EphemeralSpec` — typed wrapper that authors `(defephemeral …)`.
///
/// Lowers to a `ProcessSpec` via `From<EphemeralSpec>` — the bridge is
/// pure-typed, no string substitution. Defaults to `point_type = Gate`,
/// `substrate = Compute`, `data_classification = Internal` — every field
/// can be overridden via the full `(defpoint …)` form when the operator
/// needs the lower-level surface.
#[derive(DeriveTataraDomain, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defephemeral")]
pub struct EphemeralSpec {
    /// The Aplicacao chart + profile + overlay to install.
    pub aplicacao: AplicacaoIntent,

    /// TTL — `humantime` duration (`"1h"`, `"30m"`).
    #[serde(default = "default_ttl")]
    pub ttl: String,

    /// When the ephemeral Process auto-terminates.
    #[serde(default)]
    pub teardown: TeardownPolicy,

    /// Cluster-wide concurrency budget across ephemeral Processes sharing
    /// the same `:aplicacao :chart-ref`. `0` = no cap.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: u32,

    /// Boundary postconditions evaluated before reaching `Attested`.
    /// Typically `HelmReleaseReleased` plus one or more `ClosedLoopAuth`
    /// / `JobAttested` checks for test suites + closed-loop probes.
    #[serde(default)]
    pub postconditions: Vec<Condition>,

    /// Optional boundary preconditions (Namespace, Issuer, PullSecret
    /// readiness etc.).
    #[serde(default)]
    pub preconditions: Vec<Condition>,

    /// VERIFY-phase timeout. Empty = controller default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_timeout: Option<String>,

    /// Optional Process classification override. When omitted, defaults
    /// to `Gate / Compute / Internal / Bounded / NonMonotone`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classification: Option<Classification>,

    /// Optional parent PID path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,

    /// Declared exports — sugar that propagates through to
    /// `lifetime.ephemeral.exports` on the lowered `ProcessSpec`.
    /// Default empty = zero-trace ephemeral (nothing survives
    /// teardown). See [`crate::export`] for the full type.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<ExportSpec>,
}

fn default_ttl() -> String {
    "1h".to_string()
}
fn default_max_concurrent() -> u32 {
    1
}

impl From<EphemeralSpec> for ProcessSpec {
    fn from(e: EphemeralSpec) -> Self {
        let classification = e.classification.unwrap_or_else(default_ephemeral_class);
        let mut spec = Self {
            identity: crate::spec::IdentitySpec {
                parent: e.parent,
                name_override: None,
            },
            classification,
            intent: Intent {
                aplicacao: Some(e.aplicacao),
                ..Intent::default()
            },
            boundary: Boundary {
                preconditions: e.preconditions,
                postconditions: e.postconditions,
                timeout: e.verify_timeout,
            },
            compliance: Default::default(),
            depends_on: vec![],
            signals: Default::default(),
            lifetime: Lifetime {
                ephemeral: Some(EphemeralLifetime {
                    ttl: e.ttl,
                    teardown_policy: e.teardown,
                    max_concurrent: e.max_concurrent,
                    exports: e.exports,
                }),
                ..Lifetime::default()
            },
            suspended: false,
        };
        // Belt-and-suspenders: make sure exactly-one Intent invariant holds.
        spec.intent.nix = None;
        spec.intent.flux = None;
        spec.intent.lisp = None;
        spec.intent.container = None;
        spec.intent.guest = None;
        spec
    }
}

fn default_ephemeral_class() -> Classification {
    Classification {
        point_type: ConvergencePointType::Gate,
        substrate: SubstrateType::Compute,
        horizon: Horizon::default(),
        calm: Default::default(),
        data_classification: DataClassification::default(),
    }
}

/// Compile a `(defephemeral …)` Lisp source into named `EphemeralSpec` values.
pub fn compile_ephemeral_source(
    src: &str,
) -> tatara_lisp::Result<Vec<tatara_lisp::NamedDefinition<EphemeralSpec>>> {
    tatara_lisp::compile_named::<EphemeralSpec>(src)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boundary::ConditionKind;
    use crate::intent::IntentVariant;
    use crate::lifetime::LifetimeVariant;

    fn akeyless_overlay() -> AplicacaoIntent {
        AplicacaoIntent {
            chart_ref: "oci://ghcr.io/pleme-io/charts/lareira-akeyless-deployment".into(),
            version: "0.5.5".into(),
            profile: "gateway-with-internal-saas".into(),
            values_overlay: serde_json::json!({
                "cluster": { "name": "ephemeral-test-01", "namespace": "akeyless-test" },
                "data": { "mysql": { "persistence": { "enabled": false } } },
                "compliance": { "overlays": [] }
            }),
            release_name: Some("akeyless-saas-consolidated".into()),
            target_namespace: Some("akeyless-test".into()),
            install_timeout: Some("25m".into()),
        }
    }

    #[test]
    fn defaults_resolve_for_ephemeral_spec() {
        let e = EphemeralSpec {
            aplicacao: akeyless_overlay(),
            ttl: default_ttl(),
            teardown: TeardownPolicy::default(),
            max_concurrent: default_max_concurrent(),
            postconditions: vec![],
            preconditions: vec![],
            verify_timeout: None,
            classification: None,
            parent: None,
            exports: vec![],
        };
        let ps: ProcessSpec = e.into();
        // Intent must resolve to Aplicacao.
        match ps.intent.variant().unwrap() {
            IntentVariant::Aplicacao(a) => {
                assert_eq!(a.profile, "gateway-with-internal-saas");
                assert_eq!(a.install_timeout.as_deref(), Some("25m"));
            }
            other => panic!("expected Aplicacao, got {other:?}"),
        }
        // Lifetime must resolve to Ephemeral with defaults.
        match ps.lifetime.variant().unwrap() {
            LifetimeVariant::Ephemeral(e) => {
                assert_eq!(e.ttl, "1h");
                assert_eq!(e.teardown_policy, TeardownPolicy::Always);
            }
            other => panic!("expected ephemeral, got {other:?}"),
        }
        // Default classification gates the Process at Compute/Internal.
        assert_eq!(ps.classification.point_type, ConvergencePointType::Gate);
        assert_eq!(ps.classification.substrate, SubstrateType::Compute);
    }

    #[test]
    fn ephemeral_lisp_round_trip() {
        let src = r#"
            (defephemeral akeyless-closed-loop-attest
              :aplicacao (:chart-ref "oci://ghcr.io/pleme-io/charts/lareira-akeyless-deployment"
                          :version "0.5.5"
                          :profile "gateway-with-internal-saas"
                          :values-overlay (:cluster (:name "ephemeral-test-01")
                                           :data (:mysql (:persistence (:enabled #f)))
                                           :compliance (:overlays []))
                          :release-name "akeyless-saas-consolidated"
                          :target-namespace "akeyless-test"
                          :install-timeout "25m")
              :ttl "1h"
              :teardown OnAttested
              :max-concurrent 1
              :postconditions
                ((:kind HelmReleaseReleased
                  :params (:name "akeyless-saas-consolidated"
                           :namespace "akeyless-test"))
                 (:kind ClosedLoopAuth
                  :params (:issuer (:service "akeyless-saas-akeyless-gator" :port 8080)
                           :consumer (:service "akeyless-saas-akeyless-gateway" :port 8000)
                           :probeImage "ghcr.io/pleme-io/closed-loop-probe:0.1.0"))))
        "#;
        let defs = compile_ephemeral_source(src).expect("compile");
        assert_eq!(defs.len(), 1);
        let d = &defs[0];
        assert_eq!(d.name, "akeyless-closed-loop-attest");

        // Aplicacao body landed correctly.
        assert_eq!(
            d.spec.aplicacao.chart_ref,
            "oci://ghcr.io/pleme-io/charts/lareira-akeyless-deployment"
        );
        assert_eq!(d.spec.aplicacao.profile, "gateway-with-internal-saas");
        assert_eq!(
            d.spec.aplicacao.target_namespace.as_deref(),
            Some("akeyless-test")
        );
        // values-overlay JSON is preserved.
        assert_eq!(
            d.spec.aplicacao.values_overlay["cluster"]["name"],
            "ephemeral-test-01"
        );
        // Boolean #f is preserved as a typed JSON bool (not the string "false").
        // tatara-lisp uses Scheme syntax for bools — `#t` / `#f`.
        assert_eq!(
            d.spec.aplicacao.values_overlay["data"]["mysql"]["persistence"]["enabled"],
            false
        );

        // Lifetime knobs.
        assert_eq!(d.spec.ttl, "1h");
        assert_eq!(d.spec.teardown, TeardownPolicy::OnAttested);
        assert_eq!(d.spec.max_concurrent, 1);

        // Two postconditions, both typed.
        assert_eq!(d.spec.postconditions.len(), 2);
        assert_eq!(
            d.spec.postconditions[0].kind,
            ConditionKind::HelmReleaseReleased
        );
        assert_eq!(
            d.spec.postconditions[1].kind,
            ConditionKind::ClosedLoopAuth
        );

        // Lowers to ProcessSpec with the right shape.
        let ps: ProcessSpec = d.spec.clone().into();
        assert!(matches!(
            ps.intent.variant().unwrap(),
            IntentVariant::Aplicacao(_)
        ));
        assert!(matches!(
            ps.lifetime.variant().unwrap(),
            LifetimeVariant::Ephemeral(_)
        ));
        assert_eq!(ps.boundary.postconditions.len(), 2);
    }

    /// End-to-end: the `:exports` slot on `(defephemeral …)` compiles
    /// into typed `ExportSpec` values via the Universal-Deserialize
    /// fallthrough — no per-domain keyword handlers needed.
    ///
    /// Receipts (empty-body source) is exercised via the Rust serde
    /// path only (see `export::tests::export_spec_serde_round_trip`).
    /// tatara-lisp's empty-kw-form `(:)` currently parses as a single-
    /// element array rather than a JSON `{}`; the same limitation
    /// affects `(:permanent)` on Lifetime. Tracked: extend the reader
    /// to accept `(:foo (:))` ⇒ `{"foo": {}}` as a typed-empty form,
    /// then re-enable Receipts here.
    #[test]
    fn exports_lisp_round_trip() {
        use crate::export::{ArtifactVariant, ChannelVariant, ExportTrigger, ReportFormat};
        let src = r#"
            (defephemeral akeyless-closed-loop-attest
              :aplicacao (:chart-ref "oci://x"
                          :version "1.0.0"
                          :profile "minimal"
                          :values-overlay ())
              :ttl "30m"
              :teardown OnAttested
              :exports
                ((:source  (:test-report (:configmap "junit-results"
                                          :key       "junit.xml"
                                          :format    Junit))
                  :channel (:nats-subject (:subject "pleme.pleme-dev.ephemeral.r1.test-report"
                                           :stream  "EPHEMERAL_TEST_REPORTS"))
                  :when    OnAttested)
                 (:source  (:test-report (:configmap "junit-results"
                                          :key       "junit.xml"
                                          :format    Junit))
                  :channel (:http-event (:signal-type "test-report"))
                  :when    Always)
                 (:source  (:run-marker (:labels (:run-id "r1" :phase "end")))
                  :channel (:http-event (:signal-type "ephemeral-marker"))
                  :when    Always)))
        "#;
        let defs = compile_ephemeral_source(src).expect("compile");
        assert_eq!(defs.len(), 1);
        let d = &defs[0];
        assert_eq!(d.spec.exports.len(), 3);

        // First export — TestReport → NATS subject + OnAttested
        let r = &d.spec.exports[0];
        match r.source.variant().unwrap() {
            ArtifactVariant::TestReport(tr) => {
                assert_eq!(tr.configmap, "junit-results");
                assert_eq!(tr.format, ReportFormat::Junit);
            }
            other => panic!("expected TestReport, got {other:?}"),
        }
        match r.channel.variant().unwrap() {
            ChannelVariant::NatsSubject(n) => {
                assert_eq!(n.subject, "pleme.pleme-dev.ephemeral.r1.test-report");
                assert_eq!(n.stream, "EPHEMERAL_TEST_REPORTS");
            }
            other => panic!("expected NatsSubject, got {other:?}"),
        }
        assert_eq!(r.when, ExportTrigger::OnAttested);

        // Second export — TestReport → HTTP + Always
        let t = &d.spec.exports[1];
        match t.channel.variant().unwrap() {
            ChannelVariant::HttpEvent(h) => assert_eq!(h.signal_type, "test-report"),
            other => panic!("expected HttpEvent, got {other:?}"),
        }
        assert_eq!(t.when, ExportTrigger::Always);

        // Third export — RunMarker (BTreeMap<String,String> round-trip).
        // tatara-lisp lowercases + normalizes keyword keys before
        // handing off to serde_json — kebab `:run-id` may land as
        // either `run-id` or `runId` depending on the reader path.
        // Accept either; the round-trip property under test is
        // "label survives compile" not "exact case-form".
        let m = &d.spec.exports[2];
        match m.source.variant().unwrap() {
            ArtifactVariant::RunMarker(rm) => {
                assert_eq!(rm.labels.len(), 2);
                let run_id = rm
                    .labels
                    .get("run-id")
                    .or_else(|| rm.labels.get("runId"))
                    .or_else(|| rm.labels.get("run_id"))
                    .expect("run-id label present under some normalization");
                assert_eq!(run_id, "r1");
                assert_eq!(rm.labels.get("phase").map(String::as_str), Some("end"));
            }
            other => panic!("expected RunMarker, got {other:?}"),
        }

        // Lowered ProcessSpec carries the exports through unchanged.
        let ps: ProcessSpec = d.spec.clone().into();
        assert_eq!(ps.lifetime.ephemeral.as_ref().unwrap().exports.len(), 3);
    }

    #[test]
    fn from_impl_clears_other_intent_variants() {
        // Even if someone constructs an EphemeralSpec by hand and the
        // resulting ProcessSpec is later mutated, the From bridge sets
        // every non-Aplicacao slot to None explicitly.
        let e = EphemeralSpec {
            aplicacao: akeyless_overlay(),
            ttl: "10m".into(),
            teardown: TeardownPolicy::Never,
            max_concurrent: 0,
            postconditions: vec![],
            preconditions: vec![],
            verify_timeout: None,
            classification: None,
            parent: Some("seph.1".into()),
            exports: vec![],
        };
        let ps: ProcessSpec = e.into();
        assert!(ps.intent.nix.is_none());
        assert!(ps.intent.flux.is_none());
        assert!(ps.intent.lisp.is_none());
        assert!(ps.intent.container.is_none());
        assert!(ps.intent.guest.is_none());
        assert!(ps.intent.aplicacao.is_some());
        assert_eq!(ps.identity.parent.as_deref(), Some("seph.1"));
    }
}
