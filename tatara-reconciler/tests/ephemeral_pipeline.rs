//! End-to-end ephemeral pipeline integration test.
//!
//! This test stands as the template for any future ephemeral consumer
//! in pleme-io. It exercises every committed primitive in one file:
//!
//!   1.  `(defephemeral …)` Lisp form
//!   2.  → typed `EphemeralSpec` via the TataraDomain derive
//!   3.  → `ProcessSpec` via `From<EphemeralSpec>` (typed bridge)
//!   4.  → wrapped in a `Process` CR with deterministic identity
//!   5.  → `tatara_reconciler::render::render` emits an `OCIRepository`
//!        + `HelmRelease` with profile injected into values
//!   6.  → a Job emits a `ReceiptEnvelope` (closed-loop-auth kind)
//!   7.  → reconciler verifier accepts the receipt (verify_root + JSON
//!        round-trip), the receipt lowers to a `ProcessAttestation`,
//!        the chain extends, all hashes are byte-exact.
//!
//! Future ephemeral consumer (any future product that wants closed-loop
//! attestation): copy this file, swap the chart-ref / profile /
//! postconditions / receipt kind. Everything else composes for free.

use tatara_process::ephemeral::compile_ephemeral_source;
use tatara_process::intent::IntentVariant;
use tatara_process::lifetime::{LifetimeVariant, TeardownPolicy};
use tatara_process::prelude::{Process, ProcessSpec};
use tatara_process::receipt::{ReceiptEnvelope, ReceiptKind, RECEIPT_VERSION};

use tatara_reconciler::render;

// Boolean literals in tatara-lisp use Scheme `#t` / `#f` (not
// `true` / `false`, which the reader treats as symbols → strings).
const SAMPLE_FORM: &str = r#"
    (defephemeral akeyless-closed-loop-attest
      :aplicacao (:chart-ref "oci://ghcr.io/pleme-io/charts/lareira-akeyless-deployment"
                  :version "0.5.5"
                  :profile "gateway-with-internal-saas"
                  :values-overlay (:cluster (:name "ephemeral-test-01")
                                   :data (:mysql (:persistence (:enabled #f)))
                                   :compliance (:overlays [])
                                   :closedLoopProbe (:enabled #t))
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

#[test]
fn ephemeral_lisp_form_round_trips_through_full_pipeline() {
    // 1 + 2 — Lisp → EphemeralSpec.
    let defs = compile_ephemeral_source(SAMPLE_FORM).expect("compile");
    assert_eq!(defs.len(), 1);
    let ephemeral = defs.into_iter().next().unwrap();
    assert_eq!(ephemeral.name, "akeyless-closed-loop-attest");
    assert_eq!(ephemeral.spec.teardown, TeardownPolicy::OnAttested);
    assert_eq!(ephemeral.spec.ttl, "1h");
    assert_eq!(ephemeral.spec.postconditions.len(), 2);

    // 3 — EphemeralSpec → ProcessSpec.
    let mut spec: ProcessSpec = ephemeral.spec.clone().into();
    // Sanity: lifetime + intent resolved.
    assert!(matches!(
        spec.lifetime.variant().unwrap(),
        LifetimeVariant::Ephemeral(_)
    ));
    assert!(matches!(
        spec.intent.variant().unwrap(),
        IntentVariant::Aplicacao(_)
    ));
    // 4 — wrap in a Process CR with metadata.
    let mut proc = Process::new(&ephemeral.name, spec.clone());
    proc.metadata.namespace = Some("akeyless-test".into());
    // Suppress: silence the unused mut warning while keeping the
    // `spec` binding mutable so future tests can extend below.
    let _ = &mut spec;

    // 5 — render → OCIRepository + HelmRelease.
    let out = render::render(&proc, &proc.spec.intent).expect("render");
    assert_eq!(
        out.resources.len(),
        2,
        "expected OCIRepository + HelmRelease"
    );
    let oci = out
        .resources
        .iter()
        .find(|r| r["kind"] == "OCIRepository")
        .expect("OCIRepository present");
    let hr = out
        .resources
        .iter()
        .find(|r| r["kind"] == "HelmRelease")
        .expect("HelmRelease present");

    assert_eq!(
        oci["spec"]["url"],
        "oci://ghcr.io/pleme-io/charts/lareira-akeyless-deployment"
    );
    assert_eq!(oci["spec"]["ref"]["tag"], "0.5.5");

    // HelmRelease references the OCIRepository by same name+namespace.
    assert_eq!(hr["spec"]["chartRef"]["kind"], "OCIRepository");
    assert_eq!(hr["spec"]["chartRef"]["name"], oci["metadata"]["name"]);

    // Profile injected into values.
    assert_eq!(
        hr["spec"]["values"]["profile"],
        "gateway-with-internal-saas"
    );
    // values_overlay carried through untouched (deeply).
    assert_eq!(
        hr["spec"]["values"]["cluster"]["name"],
        "ephemeral-test-01"
    );
    assert_eq!(
        hr["spec"]["values"]["data"]["mysql"]["persistence"]["enabled"],
        false
    );
    // The probe sub-chart is enabled via the operator's overlay.
    assert_eq!(hr["spec"]["values"]["closedLoopProbe"]["enabled"], true);

    // Owner-process annotations present on both resources so the
    // controller's reconcile loop can correlate them.
    for r in &out.resources {
        let anns = &r["metadata"]["annotations"];
        assert_eq!(
            anns[tatara_process::annotations::PROCESS],
            "akeyless-test/akeyless-closed-loop-attest"
        );
    }

    // Intent + artifact hashes deterministically derived from the
    // canonical bytes — same inputs → same hashes.
    let intent_hex = render::intent_hash(&out.intent_bytes);
    let artifact_hex = render::artifact_hash(&out.artifact_bytes);
    assert!(!intent_hex.is_empty());
    assert!(!artifact_hex.is_empty());

    // Re-render: same form must produce byte-identical hashes.
    let out2 = render::render(&proc, &proc.spec.intent).expect("re-render");
    assert_eq!(render::intent_hash(&out2.intent_bytes), intent_hex);
    assert_eq!(
        render::artifact_hash(&out2.artifact_bytes),
        artifact_hex,
        "renderer must be deterministic"
    );
}

#[test]
fn closed_loop_receipt_round_trips_and_lowers_to_attestation() {
    // 6 — Build a receipt the probe Job would emit, serialize to JSON
    // (the ConfigMap wire form), then parse back via the same typed
    // path the reconciler uses.
    let probe_receipt = ReceiptEnvelope::build(
        ReceiptKind::ClosedLoopAuth,
        "intent-hash-from-gator-jwk",
        "artifact-hash-of-secret-blob",
        "control-hash-of-signature-verify",
        None,
    );
    assert_eq!(probe_receipt.version, RECEIPT_VERSION);
    assert_eq!(probe_receipt.kind, ReceiptKind::ClosedLoopAuth.as_str());
    // And the typed projection round-trips: the wire-format kind
    // decodes back into the typed variant the probe authored with.
    assert_eq!(
        probe_receipt.known_kind(),
        Some(ReceiptKind::ClosedLoopAuth)
    );
    assert!(probe_receipt.verify_shape().is_ok());
    assert!(probe_receipt.verify_root(None));

    // JSON wire form (matches what the chart's Job writes to
    // data['receipt.json']).
    let wire = serde_json::to_string(&probe_receipt).unwrap();
    let parsed = ReceiptEnvelope::parse_either(&wire).expect("parse via either");
    assert_eq!(parsed, probe_receipt);

    // YAML wire form also works.
    let yaml = serde_yaml::to_string(&probe_receipt).unwrap();
    let parsed_yaml = ReceiptEnvelope::parse_either(&yaml).expect("parse yaml");
    assert_eq!(parsed_yaml, probe_receipt);

    // 7 — Lower the receipt into a ProcessAttestation chain.
    let initial = parsed.to_attestation(0, None);
    assert!(initial.verify());
    assert_eq!(initial.composed_root, parsed.composed_root);

    // Next attestation chains via previous_root — the next ephemeral
    // run's receipt extends the chain.
    let next_receipt = ReceiptEnvelope::build(
        ReceiptKind::ClosedLoopAuth,
        "next-intent",
        "next-artifact",
        "next-control",
        Some(&initial.composed_root),
    );
    let next = next_receipt.to_attestation(1, Some(&initial.composed_root));
    assert_eq!(next.generation, 1);
    assert_eq!(
        next.previous_root.as_deref(),
        Some(initial.composed_root.as_str())
    );
    assert!(next.verify());
    assert_ne!(next.composed_root, initial.composed_root);
}

#[test]
fn renderer_omits_helmrepository_chartref_when_oci_used() {
    // Belt-and-suspenders: OCI chart-ref must NOT emit the older
    // `chart.spec.sourceRef` block. Two competing chart selectors
    // would confuse helm-controller.
    let defs = compile_ephemeral_source(SAMPLE_FORM).expect("compile");
    let spec: ProcessSpec = defs.into_iter().next().unwrap().spec.into();
    let mut proc = Process::new("t", spec);
    proc.metadata.namespace = Some("akeyless-test".into());

    let out = render::render(&proc, &proc.spec.intent).expect("render");
    let hr = out
        .resources
        .iter()
        .find(|r| r["kind"] == "HelmRelease")
        .unwrap();
    assert!(
        hr["spec"]["chart"].is_null() || !hr["spec"]["chart"].is_object(),
        "OCI path must not also emit spec.chart.spec.sourceRef block"
    );
    assert_eq!(hr["spec"]["chartRef"]["kind"], "OCIRepository");
}
