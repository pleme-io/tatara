//! Render — `Intent` → FluxCD CRs (JSON `Value` form; the controller will
//! wrap with owner references and apply via `Api<DynamicObject>`).

use anyhow::Result;
use serde_json::{json, Value};

use tatara_process::annotations;
use tatara_process::intent::{
    AplicacaoIntent, FluxIntent, Intent, IntentVariant, LispIntent, NixIntent,
};
use tatara_process::prelude::Process;

/// Produced resources from a render pass.
#[derive(Debug, Clone)]
pub struct RenderOutput {
    /// Fully-formed FluxCD / K8s resources (as JSON), ready for `ssapply`.
    pub resources: Vec<Value>,
    /// `artifact_hash` pillar input — BLAKE3 of the canonical resource bytes.
    pub artifact_bytes: Vec<u8>,
    /// `intent_hash` pillar input — canonical spec + store path / AST bytes.
    pub intent_bytes: Vec<u8>,
}

/// Render an `Intent` into FluxCD resources owned by `process`.
pub fn render(process: &Process, intent: &Intent) -> Result<RenderOutput> {
    let variant = intent.variant()?;
    let owner_name = process
        .metadata
        .name
        .clone()
        .unwrap_or_else(|| "unnamed".into());
    let owner_ns = process
        .metadata
        .namespace
        .clone()
        .unwrap_or_else(|| "default".into());

    let (resources, intent_bytes) = match variant {
        IntentVariant::Flux(f) => render_flux(&owner_name, &owner_ns, f),
        IntentVariant::Nix(n) => render_nix(&owner_name, &owner_ns, n),
        IntentVariant::Lisp(l) => render_lisp(&owner_name, &owner_ns, l)?,
        IntentVariant::Container(_) => (vec![], vec![]),
        // Guest intents (HVF / VZ / WASM) are owned by tatara-hospedeiro —
        // the reconciler emits no K8s resources for them. Intent bytes
        // still feed the three-pillar attestation chain.
        IntentVariant::Guest(g) => (vec![], serde_json::to_vec(g).unwrap_or_default()),
        IntentVariant::Aplicacao(a) => render_aplicacao(&owner_name, &owner_ns, a),
    };

    let artifact_bytes = canonical_bytes(&resources);
    Ok(RenderOutput {
        resources,
        artifact_bytes,
        intent_bytes,
    })
}

fn render_flux(name: &str, ns: &str, f: &FluxIntent) -> (Vec<Value>, Vec<u8>) {
    // Kustomization lives in the Process's namespace so that K8s-native
    // ownerReferences (same-namespace only) cascade cleanup on deletion.
    let mut spec = serde_json::Map::new();
    spec.insert("interval".into(), Value::String("1m".into()));
    spec.insert("path".into(), Value::String(f.path.clone()));
    spec.insert("prune".into(), Value::Bool(true));
    spec.insert(
        "sourceRef".into(),
        json!({
            "kind": "GitRepository",
            "name": f.git_repository,
            "namespace": f.git_repository_namespace
                .clone()
                .unwrap_or_else(|| "flux-system".into()),
        }),
    );
    if let Some(tn) = &f.target_namespace {
        spec.insert("targetNamespace".into(), Value::String(tn.clone()));
    }
    if f.decrypt_sops {
        spec.insert(
            "decryption".into(),
            json!({ "provider": "sops", "secretRef": { "name": "sops-age" }}),
        );
    }

    let kustomization = json!({
        "apiVersion": "kustomize.toolkit.fluxcd.io/v1",
        "kind": "Kustomization",
        "metadata": {
            "name": name,
            "namespace": ns,
            "annotations": {
                annotations::MANAGED_BY: "tatara-reconciler",
                annotations::PROCESS: format!("{ns}/{name}"),
            },
        },
        "spec": Value::Object(spec),
    });

    let intent_bytes = serde_json::to_vec(f).unwrap_or_default();
    (vec![kustomization], intent_bytes)
}

/// Render an `AplicacaoIntent` to a FluxCD HelmRelease (and, for OCI chart
/// refs, an owning `OCIRepository`). Both resources live in the Process's
/// namespace so K8s-native ownerReferences cascade cleanup on Process
/// termination — load-bearing for the ephemeral teardown path.
///
/// The closed-loop discovery property (gateway → bundled SaaS over K8s
/// DNS) requires no extra wiring here: the chart's `profile:
/// gateway-with-internal-saas` already auto-derives the gator URL from
/// the release name + namespace, and we emit both into the same
/// namespace as the Process.
fn render_aplicacao(name: &str, ns: &str, a: &AplicacaoIntent) -> (Vec<Value>, Vec<u8>) {
    let release_name = a.release_name.clone().unwrap_or_else(|| name.into());
    let target_ns = a.target_namespace.clone().unwrap_or_else(|| ns.into());

    // Merge the operator's values_overlay with the profile keyword so the
    // typed `profile:` chart switch is always set when the operator
    // specified one. The overlay is JSON; we extend the top-level object.
    let mut values = match a.values_overlay.clone() {
        Value::Object(m) => m,
        Value::Null => serde_json::Map::new(),
        other => {
            // Non-object overlays are wrapped under `_overlay` so the
            // chart at least sees the value — but this is an authoring
            // mistake the caller should fix. We never silently drop.
            let mut m = serde_json::Map::new();
            m.insert("_overlay".into(), other);
            m
        }
    };
    if !a.profile.is_empty() {
        values.insert("profile".into(), Value::String(a.profile.clone()));
    }

    // Split the chart reference: OCI → emit OCIRepository + HelmRelease.chartRef.
    // Anything else is treated as `<repo-name>/<chart-name>` against a
    // pre-existing HelmRepository (operator pre-creates the repo).
    let (mut resources, chart_block) = if let Some(oci) = parse_oci_ref(&a.chart_ref) {
        let oci_repo = json!({
            "apiVersion": "source.toolkit.fluxcd.io/v1beta2",
            "kind": "OCIRepository",
            "metadata": {
                "name": name,
                "namespace": ns,
                "annotations": {
                    annotations::MANAGED_BY: "tatara-reconciler",
                    annotations::PROCESS: format!("{ns}/{name}"),
                },
            },
            "spec": {
                "interval": "5m",
                "url": oci.registry_url,
                "ref": { "tag": a.version },
            },
        });
        // HelmRelease v2 `chartRef` pointer.
        let chart_block = json!({
            "chartRef": {
                "kind": "OCIRepository",
                "name": name,
                "namespace": ns,
            },
        });
        (vec![oci_repo], chart_block)
    } else {
        // HelmRepository-style — operator must have created a HelmRepository
        // named `<chart_ref-split-prefix>` in flux-system or the Process namespace.
        let (repo, chart) = split_repo_chart(&a.chart_ref);
        let chart_block = json!({
            "chart": {
                "spec": {
                    "chart": chart,
                    "version": a.version,
                    "sourceRef": {
                        "kind": "HelmRepository",
                        "name": repo,
                        "namespace": "flux-system",
                    },
                },
            },
        });
        (vec![], chart_block)
    };

    let mut hr_spec = serde_json::Map::new();
    hr_spec.insert("interval".into(), Value::String("5m".into()));
    hr_spec.insert("releaseName".into(), Value::String(release_name.clone()));
    hr_spec.insert("targetNamespace".into(), Value::String(target_ns));
    if let Some(chart_obj) = chart_block.as_object() {
        for (k, v) in chart_obj {
            hr_spec.insert(k.clone(), v.clone());
        }
    }
    let install = json!({
        "timeout": a.install_timeout.clone().unwrap_or_else(|| "25m".into()),
        "remediation": { "retries": 3 },
    });
    hr_spec.insert("install".into(), install);
    hr_spec.insert(
        "upgrade".into(),
        json!({
            "timeout": a.install_timeout.clone().unwrap_or_else(|| "25m".into()),
            "remediation": { "retries": 3 },
        }),
    );
    hr_spec.insert("values".into(), Value::Object(values));

    let helm_release = json!({
        "apiVersion": "helm.toolkit.fluxcd.io/v2",
        "kind": "HelmRelease",
        "metadata": {
            "name": name,
            "namespace": ns,
            "annotations": {
                annotations::MANAGED_BY: "tatara-reconciler",
                annotations::PROCESS: format!("{ns}/{name}"),
            },
        },
        "spec": Value::Object(hr_spec),
    });
    resources.push(helm_release);

    let intent_bytes = serde_json::to_vec(a).unwrap_or_default();
    (resources, intent_bytes)
}

/// Parsed OCI reference — `oci://<host>/<path>/<chart>` → registry URL
/// (without `oci://` scheme is what Flux's OCIRepository.spec.url wants).
struct OciRef {
    registry_url: String,
}

fn parse_oci_ref(s: &str) -> Option<OciRef> {
    if let Some(rest) = s.strip_prefix("oci://") {
        // Flux OCIRepository wants the full `oci://host/path` URL.
        Some(OciRef {
            registry_url: ["oci://", rest].concat(),
        })
    } else {
        None
    }
}

/// Split `repo-name/chart-name` (HelmRepository style) into its parts.
/// If no slash, treat the entire string as the chart name and use
/// `default` as the repo.
fn split_repo_chart(s: &str) -> (String, String) {
    match s.split_once('/') {
        Some((repo, chart)) => (repo.into(), chart.into()),
        None => ("default".into(), s.into()),
    }
}

fn render_nix(_name: &str, _ns: &str, n: &NixIntent) -> (Vec<Value>, Vec<u8>) {
    // TODO: hand off to tatara-engine nix_eval driver (or delegate via NixBuild CRD
    // when `n.delegate_to_nix_build == true`) and then wrap the resulting resource
    // set in an emitted Kustomization pointing at a controller-managed path.
    let intent_bytes = serde_json::to_vec(n).unwrap_or_default();
    (vec![], intent_bytes)
}

fn render_lisp(_name: &str, _ns: &str, l: &LispIntent) -> Result<(Vec<Value>, Vec<u8>)> {
    // Parse the Lisp source — an AST-form intent_hash input even if
    // macroexpansion has not yet landed.
    let forms = tatara_lisp::read(&l.source)?;
    let ast_bytes = serde_json::to_vec(&forms.iter().map(|f| f.to_string()).collect::<Vec<_>>())
        .unwrap_or_default();
    // TODO: macroexpand `(defpoint ...)` forms → compile to ProcessSpec or resources.
    Ok((vec![], ast_bytes))
}

fn canonical_bytes(resources: &[Value]) -> Vec<u8> {
    let mut out = Vec::new();
    for r in resources {
        if let Ok(bytes) = serde_json::to_vec(r) {
            out.extend_from_slice(&bytes);
            out.push(b'\n');
        }
    }
    out
}

/// Compute the `artifact_hash` pillar from canonical resource bytes.
pub fn artifact_hash(bytes: &[u8]) -> String {
    hex::encode(blake3::hash(bytes).as_bytes())
}

/// Compute the `intent_hash` pillar from canonical intent bytes.
pub fn intent_hash(bytes: &[u8]) -> String {
    hex::encode(blake3::hash(bytes).as_bytes())
}

#[cfg(test)]
mod aplicacao_tests {
    use super::*;
    use tatara_process::intent::AplicacaoIntent;

    fn akeyless_intent() -> AplicacaoIntent {
        AplicacaoIntent {
            chart_ref: "oci://ghcr.io/pleme-io/charts/lareira-akeyless-deployment".into(),
            version: "0.5.5".into(),
            profile: "gateway-with-internal-saas".into(),
            values_overlay: serde_json::json!({
                "cluster": { "name": "ephemeral-test-01" },
                "data": { "mysql": { "persistence": { "enabled": false } } },
                "compliance": { "overlays": [] }
            }),
            release_name: Some("akeyless-saas-consolidated".into()),
            target_namespace: Some("akeyless-test".into()),
            install_timeout: Some("25m".into()),
        }
    }

    #[test]
    fn oci_emits_ocirepository_plus_helmrelease() {
        let (resources, intent_bytes) =
            render_aplicacao("ephemeral-akeyless", "akeyless-test", &akeyless_intent());
        assert_eq!(resources.len(), 2);
        assert_eq!(resources[0]["kind"], "OCIRepository");
        assert_eq!(resources[1]["kind"], "HelmRelease");
        // OCIRepository in Process's namespace + name → same as Process.
        assert_eq!(resources[0]["metadata"]["name"], "ephemeral-akeyless");
        assert_eq!(resources[0]["metadata"]["namespace"], "akeyless-test");
        assert_eq!(
            resources[0]["spec"]["url"],
            "oci://ghcr.io/pleme-io/charts/lareira-akeyless-deployment"
        );
        assert_eq!(resources[0]["spec"]["ref"]["tag"], "0.5.5");
        // HelmRelease references the OCIRepository via chartRef.
        assert_eq!(
            resources[1]["spec"]["chartRef"]["kind"],
            "OCIRepository"
        );
        assert_eq!(
            resources[1]["spec"]["chartRef"]["name"],
            "ephemeral-akeyless"
        );
        // releaseName + targetNamespace honored.
        assert_eq!(
            resources[1]["spec"]["releaseName"],
            "akeyless-saas-consolidated"
        );
        assert_eq!(
            resources[1]["spec"]["targetNamespace"],
            "akeyless-test"
        );
        // profile injected into values (typed switch for the chart).
        assert_eq!(
            resources[1]["spec"]["values"]["profile"],
            "gateway-with-internal-saas"
        );
        // Values overlay carried through untouched.
        assert_eq!(
            resources[1]["spec"]["values"]["cluster"]["name"],
            "ephemeral-test-01"
        );
        // Install timeout honored.
        assert_eq!(resources[1]["spec"]["install"]["timeout"], "25m");
        // Intent bytes deterministic + non-empty.
        assert!(!intent_bytes.is_empty());
    }

    #[test]
    fn target_namespace_defaults_to_process_namespace() {
        let mut a = akeyless_intent();
        a.target_namespace = None;
        a.release_name = None;
        let (resources, _) = render_aplicacao("test-proc", "my-ns", &a);
        let hr = resources.iter().find(|r| r["kind"] == "HelmRelease").unwrap();
        assert_eq!(hr["spec"]["targetNamespace"], "my-ns");
        assert_eq!(hr["spec"]["releaseName"], "test-proc");
    }

    #[test]
    fn install_timeout_defaults_to_25m() {
        let mut a = akeyless_intent();
        a.install_timeout = None;
        let (resources, _) = render_aplicacao("p", "ns", &a);
        let hr = resources.iter().find(|r| r["kind"] == "HelmRelease").unwrap();
        assert_eq!(hr["spec"]["install"]["timeout"], "25m");
        assert_eq!(hr["spec"]["install"]["remediation"]["retries"], 3);
    }

    #[test]
    fn helmrepository_chartref_for_non_oci() {
        let a = AplicacaoIntent {
            chart_ref: "pleme-io/lareira-akeyless-deployment".into(),
            version: "0.5.5".into(),
            profile: String::new(),
            values_overlay: serde_json::Value::Null,
            release_name: None,
            target_namespace: None,
            install_timeout: None,
        };
        let (resources, _) = render_aplicacao("p", "ns", &a);
        // No OCIRepository — just a HelmRelease pointing at a HelmRepository.
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0]["kind"], "HelmRelease");
        assert_eq!(
            resources[0]["spec"]["chart"]["spec"]["chart"],
            "lareira-akeyless-deployment"
        );
        assert_eq!(
            resources[0]["spec"]["chart"]["spec"]["sourceRef"]["name"],
            "pleme-io"
        );
        // Empty profile → not injected as a values key.
        let values = &resources[0]["spec"]["values"];
        assert!(values.get("profile").is_none() || values["profile"].is_null());
    }

    #[test]
    fn process_annotations_carry_owner_path() {
        let a = akeyless_intent();
        let (resources, _) = render_aplicacao("ephemeral-akeyless", "akeyless-test", &a);
        for r in &resources {
            let anns = &r["metadata"]["annotations"];
            assert_eq!(anns[tatara_process::annotations::MANAGED_BY], "tatara-reconciler");
            assert_eq!(
                anns[tatara_process::annotations::PROCESS],
                "akeyless-test/ephemeral-akeyless"
            );
        }
    }

    #[test]
    fn render_through_top_level_intent_dispatch() {
        // End-to-end: a ProcessSpec with Intent::Aplicacao routes through
        // the top-level `render()` function.
        use kube::Resource;
        use tatara_process::prelude::{Process, ProcessSpec};

        let intent = tatara_process::intent::Intent {
            aplicacao: Some(akeyless_intent()),
            ..tatara_process::intent::Intent::default()
        };
        let spec = ProcessSpec {
            identity: Default::default(),
            classification: tatara_process::classification::Classification {
                point_type: tatara_process::classification::ConvergencePointType::Gate,
                substrate: tatara_process::classification::SubstrateType::Compute,
                horizon: Default::default(),
                calm: Default::default(),
                data_classification: Default::default(),
            },
            intent: intent.clone(),
            boundary: Default::default(),
            compliance: Default::default(),
            depends_on: vec![],
            signals: Default::default(),
            lifetime: Default::default(),
            suspended: false,
        };
        let mut proc = Process::new("ephemeral-akeyless", spec);
        proc.meta_mut().namespace = Some("akeyless-test".into());
        let out = render(&proc, &intent).expect("render");
        assert_eq!(out.resources.len(), 2);
        assert!(!out.intent_bytes.is_empty());
        assert!(!out.artifact_bytes.is_empty());
    }

    #[test]
    fn parse_oci_ref_works() {
        let r = parse_oci_ref("oci://ghcr.io/pleme-io/charts/foo").unwrap();
        assert_eq!(r.registry_url, "oci://ghcr.io/pleme-io/charts/foo");
        assert!(parse_oci_ref("ghcr.io/pleme-io/charts/foo").is_none());
        assert!(parse_oci_ref("pleme-io/foo").is_none());
    }

    #[test]
    fn split_repo_chart_handles_missing_slash() {
        assert_eq!(
            split_repo_chart("pleme-io/foo-chart"),
            ("pleme-io".into(), "foo-chart".into())
        );
        assert_eq!(
            split_repo_chart("loose-chart-name"),
            ("default".into(), "loose-chart-name".into())
        );
    }
}
