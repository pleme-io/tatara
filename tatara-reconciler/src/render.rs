//! Render — `Intent` → FluxCD CRs (JSON `Value` form; the controller will
//! wrap with owner references and apply via `Api<DynamicObject>`).

use anyhow::Result;
use serde_json::{json, Value};

use tatara_process::annotations;
use tatara_process::export::ExportSpec;
use tatara_process::hostname::{
    ephemeral_id_from_spec, fmt_fqdn, fmt_fqdn_stable, resolve_ephemeral_id,
};
use tatara_process::intent::{
    AplicacaoIntent, FluxIntent, Intent, IntentVariant, LispIntent, NixIntent,
};
use tatara_process::phase::ProcessPhase;
use tatara_process::prelude::Process;
use tatara_process::routing::RoutingSpec;

use crate::edges::{DnsEndpointEdge, Edge, EdgeContext, IngressEdge};

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
    // Owner-metadata seed threaded through every per-Intent render
    // arm below. The `(ns, name)` pair rides through the substrate
    // `Process::coordinates_or_defaults` primitive so the same
    // workspace-wide fallback strings ("default" / "unnamed") that
    // the SSA-time re-injection (ssapply::inject_annotations),
    // claim-arbiter row builder (table_controller::reconcile), the
    // sibling routing renderer (render_routing), and the boundary-
    // evaluator default-namespace resolver all substitute land here
    // too — a rename of either fallback (or a future normalization
    // sweep) reaches this seed through ONE primitive.
    let (owner_ns, owner_name) = process.coordinates_or_defaults();

    // R12 — Encapsulation mode dispatch.
    //
    // Process.encapsulates.mode controls how Intent renders:
    //   * Manage (default / None)  → emit greenfield resources
    //   * Adopt                    → emit HR with releaseName matching
    //                                the pre-existing release; helm-
    //                                controller adopts it in place
    //   * Observe                  → emit NOTHING here; the Process
    //                                only watches + adds routing/
    //                                exports/attestation
    //
    // The two branches below route through the typed projections
    // (`emits_workload` / `preserves_release_name`) on
    // `EncapsulationMode` rather than `mode == Variant` equality —
    // adding a fourth variant in `tatara-process` reaches each
    // projection's closed-set match, not these sites.
    use tatara_process::encapsulates::EncapsulationMode;
    let mode = process
        .spec
        .encapsulates
        .as_ref()
        .map(|e| e.mode)
        .unwrap_or(EncapsulationMode::Manage);

    let (resources, intent_bytes) = if !mode.emits_workload() {
        // Observe mode — no Intent-driven workload emission.
        // Intent bytes still go into the attestation pillar so the
        // typed shape the Process declares is recorded. ONE site
        // owns the per-variant serialization via
        // `IntentVariant::canonical_bytes` — adding a 7th variant
        // extends that method, not this dispatcher.
        (vec![], variant.canonical_bytes())
    } else {
        match variant {
            IntentVariant::Flux(f) => render_flux(owner_name, owner_ns, f),
            IntentVariant::Nix(n) => render_nix(owner_name, owner_ns, n),
            IntentVariant::Lisp(l) => render_lisp(owner_name, owner_ns, l)?,
            IntentVariant::Container(_) => (vec![], vec![]),
            // Guest intents (HVF / VZ / WASM) are owned by tatara-hospedeiro —
            // the reconciler emits no K8s resources for them. Intent bytes
            // still feed the three-pillar attestation chain.
            IntentVariant::Guest(g) => (vec![], serde_json::to_vec(g).unwrap_or_default()),
            IntentVariant::Aplicacao(a) => {
                // Adopt mode is implicit: render_aplicacao already uses
                // `a.release_name` as the HelmRelease releaseName when
                // set; in Adopt mode the operator sets release_name to
                // match the pre-existing release, helm-controller does
                // the rest. R12 adds an adoption annotation so the
                // operator can see at-a-glance which HRs are adopting.
                let (resources, bytes) = render_aplicacao(owner_name, owner_ns, a);
                let resources = if mode.preserves_release_name() {
                    mark_resources_as_adopting(resources, process)
                } else {
                    resources
                };
                (resources, bytes)
            }
        }
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
            "annotations": crate::ssapply::ownership_annotations(&crate::ssapply::qualified_process_ref(ns, name)),
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
/// The closed-loop discovery property (client → bundled issuer over K8s
/// DNS) requires no extra wiring here: the chart's `profile:
/// all-in-one` already auto-derives the issuer URL from
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
                "annotations": crate::ssapply::ownership_annotations(&crate::ssapply::qualified_process_ref(ns, name)),
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
    // Flux `HelmRelease.spec.{install,upgrade}` — both slots carry
    // BYTE-IDENTICAL policies today, derived off the enclosing
    // `AplicacaoIntent` via the substrate primitive
    // `AplicacaoIntent::helm_lifecycle_policy`. Pre-lift this was
    // two adjacent hand-authored `json!({"timeout": …,
    // "remediation": {"retries": 3}})` blocks past the ★★ PRIME-
    // DIRECTIVE ≥ 2 duplication threshold; post-lift both slots
    // ride through ONE typed composer on the owning intent, and
    // a future two-slot split (distinct install vs upgrade
    // policies, a `wait: bool` slot, a `disableOpenAPIValidation`
    // slot) lands at the primitive's shape, not at this callsite.
    // Pinned byte-identical here by
    // `helmrelease_install_and_upgrade_carry_byte_identical_lifecycle_policy`.
    let lifecycle = serde_json::to_value(a.helm_lifecycle_policy())
        .expect("HelmLifecyclePolicy → Value never fails: plain (String, u8) struct");
    hr_spec.insert("install".into(), lifecycle.clone());
    hr_spec.insert("upgrade".into(), lifecycle);
    hr_spec.insert("values".into(), Value::Object(values));

    let helm_release = json!({
        "apiVersion": "helm.toolkit.fluxcd.io/v2",
        "kind": "HelmRelease",
        "metadata": {
            "name": name,
            "namespace": ns,
            "annotations": crate::ssapply::ownership_annotations(&crate::ssapply::qualified_process_ref(ns, name)),
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

/// Stamp every emitted resource with a `tatara.pleme.io/encapsulation-mode`
/// annotation (= "Adopt") + a back-reference annotation pointing at
/// the existing HR's `releaseName` so operators can see at-a-glance
/// which HRs are adopting which pre-existing releases.
fn mark_resources_as_adopting(resources: Vec<Value>, process: &Process) -> Vec<Value> {
    use tatara_process::encapsulates::{EncapsulationKindVariant, ExistingHelmRelease};
    let adoption_ref: Option<&ExistingHelmRelease> =
        process
            .spec
            .encapsulates
            .as_ref()
            .and_then(|e| match e.kind.variant().ok() {
                Some(EncapsulationKindVariant::ExistingHelmRelease(h)) => Some(h),
                _ => None,
            });
    resources
        .into_iter()
        .map(|mut r| {
            if let Some(meta) = r.as_object_mut().and_then(|o| o.get_mut("metadata")) {
                if let Some(meta_obj) = meta.as_object_mut() {
                    let anns = meta_obj
                        .entry("annotations")
                        .or_insert_with(|| Value::Object(serde_json::Map::new()));
                    if let Some(anns_obj) = anns.as_object_mut() {
                        anns_obj.insert(
                            "tatara.pleme.io/encapsulation-mode".into(),
                            Value::String("Adopt".into()),
                        );
                        if let Some(adopt) = adoption_ref {
                            anns_obj.insert(
                                "tatara.pleme.io/adopted-release".into(),
                                Value::String(format!(
                                    "{}/{}",
                                    adopt.namespace, adopt.release_name
                                )),
                            );
                        }
                    }
                }
            }
            r
        })
        .collect()
}

// ─── R8: Routing emission ──────────────────────────────────────────

/// Render the routing edges declared on a Process. One call =
/// every Ingress + DNSEndpoint the Process should own.
///
/// For each `RoutingSpec.hostnames` entry, emits resources via
/// every registered [`Edge`]:
///
/// * Always: per-instance form FQDN.
/// * When `stable_name_claim && holds_stable_claim`: ALSO the
///   stable form (no `ephemeral_id` segment).
///
/// `holds_stable_claim` is computed by the claim arbiter (R10) and
/// passed in by the caller — `render_routing` itself is pure on
/// `(process, routing, claim_state, dns_lb_target)`.
pub fn render_routing(
    process: &Process,
    routing: &RoutingSpec,
    holds_stable_claim: bool,
    cluster: &str,
    location: &str,
    domain: &str,
    dns_lb_target: Option<&str>,
) -> Result<Vec<Value>> {
    // Route the two-slot metadata pull through the substrate primitive
    // on `Process` — the pre-lift hand-authored fallback pair now
    // shares ONE owner with the render-time authoring seed (render),
    // the SSA-time re-injection (ssapply::inject_annotations), the
    // claim-arbiter row builder (table_controller::reconcile), and
    // the boundary-evaluator default-namespace resolver. The
    // `<ns>/<name>` composition below then rides through the
    // paired-composer substrate `qualified_process_ref` so the shape
    // convention is owned at ONE site too.
    let (process_namespace, process_name) = process.coordinates_or_defaults();
    let process_uid = process.metadata.uid.as_deref().unwrap_or("");
    let process_ref = crate::ssapply::qualified_process_ref(process_namespace, process_name);

    // Content-hash form of ephemeral_id — derived once per Process,
    // reused across every hostname on this Process.
    let fallback_hash = ephemeral_id_from_spec(&process.spec)
        .map_err(|e| anyhow::anyhow!("ephemeral_id_from_spec: {e}"))?;

    // Per-Edge handlers — the trait object list is the substrate
    // extension point. New edge target ⇒ one new impl + one entry.
    let edges: Vec<Box<dyn Edge>> = vec![
        Box::new(IngressEdge),
        Box::new(DnsEndpointEdge {
            ingress_lb_target: dns_lb_target.map(String::from),
            ttl_seconds: 60,
        }),
    ];

    let mut out: Vec<Value> = Vec::new();
    for hostname in &routing.hostnames {
        let host_cluster = hostname.cluster.as_deref().unwrap_or(cluster);
        let eph_id = resolve_ephemeral_id(hostname, &fallback_hash);

        // (1) Per-instance form — always emitted.
        let fqdn = fmt_fqdn(&hostname.app, eph_id, host_cluster, location, domain)
            .map_err(|e| anyhow::anyhow!("fmt_fqdn (per-instance): {e}"))?;
        let ctx = EdgeContext {
            process_name,
            process_namespace,
            process_uid,
            process_ref: &process_ref,
            hostname,
            ephemeral_id: eph_id,
            backend: &routing.backend,
            fqdn: &fqdn,
            is_stable: false,
        };
        for edge in &edges {
            if let Some(v) = edge.render(&ctx)? {
                out.push(v);
            }
        }

        // (2) Stable form — emitted iff Process holds the claim.
        if routing.stable_name_claim && holds_stable_claim {
            let fqdn_stable = fmt_fqdn_stable(&hostname.app, host_cluster, location, domain)
                .map_err(|e| anyhow::anyhow!("fmt_fqdn_stable: {e}"))?;
            let ctx = EdgeContext {
                process_name,
                process_namespace,
                process_uid,
                process_ref: &process_ref,
                hostname,
                ephemeral_id: eph_id,
                backend: &routing.backend,
                fqdn: &fqdn_stable,
                is_stable: true,
            };
            for edge in &edges {
                if let Some(v) = edge.render(&ctx)? {
                    out.push(v);
                }
            }
        }
    }
    Ok(out)
}

// ─── Export-worker Job rendering ───────────────────────────────────

/// Compute the canonical Job name for an `ExportSpec` at `index`
/// inside `lifetime.ephemeral.exports`. Deterministic + stable across
/// reconciles so re-applying the same spec is idempotent — the
/// reconciler creates the Job only once.
///
/// Shape: `<process-name>-export-<index>`. Stays under the 63-char
/// K8s name limit for any reasonable process name.
pub fn export_job_name(process_name: &str, index: usize) -> String {
    format!("{process_name}-export-{index}")
}

/// Canonical receipt ConfigMap name for an export Job.
///
/// Shape: `<process-name>-export-<index>-receipt`, composed as
/// `<export_job_name(process_name, index)>` ++
/// [`tatara_process::receipt::RECEIPT_CM_SUFFIX`] via
/// [`tatara_process::receipt::default_receipt_config_map_name`] so
/// the export-worker's default-derivation site shares ONE substrate
/// primitive with the reconciler's `JobAttested` +
/// `ClosedLoopAuth` default-derivation sites in
/// [`crate::boundary`]. A future rename of the `-receipt` suffix
/// lands at the ONE const on the substrate; this composer picks it
/// up mechanically.
pub fn export_receipt_configmap_name(process_name: &str, index: usize) -> String {
    tatara_process::receipt::default_receipt_config_map_name(&export_job_name(process_name, index))
}

/// Render one `batch/v1` Job per ExportSpec that fires for the
/// given terminal-reached gate (`Attested` or `Failed`).
///
/// Each Job:
///   * is owned by the Process (cascading delete on Reaped)
///   * carries labels selectable by the reconciler's `handle_releasing`
///     watch loop: `tatara.pleme.io/process={ns/name}`,
///     `tatara.pleme.io/role=export`,
///     `tatara.pleme.io/export-index={index}`
///   * runs `tatara-export-worker` from the supplied `image`
///   * passes the ExportSpec JSON as the `--spec` argv flag
///   * stamps the previous attestation root (when present on
///     `process.status.attestation.composed_root`) as
///     `--previous-root` so the receipt chains into the Process
///     attestation tree
///   * targets a receipt ConfigMap derived from `export_receipt_configmap_name`;
///     the reconciler's `JobAttested` evaluator reads that ConfigMap
///     once the Job reports Succeeded.
///
/// The function is pure — no kube client, no IO — so the JSON
/// shape is unit-testable. The caller (`handle_releasing`) applies
/// each rendered Job via `ssapply`.
pub fn render_export_jobs(
    process: &Process,
    gate: ProcessPhase,
    image: &str,
    service_account: &str,
) -> Result<Vec<Value>> {
    let ephemeral = match process.spec.lifetime.ephemeral.as_ref() {
        Some(e) => e,
        None => return Ok(vec![]),
    };
    let ns = process
        .metadata
        .namespace
        .as_deref()
        .unwrap_or("default")
        .to_string();
    let name = process
        .metadata
        .name
        .as_deref()
        .unwrap_or("unnamed")
        .to_string();
    let process_ref = crate::ssapply::qualified_process_ref(&ns, &name);
    let uid = process.metadata.uid.as_deref().unwrap_or("");
    let previous_root = process
        .status
        .as_ref()
        .and_then(|s| s.attestation.as_ref())
        .map(|a| a.composed_root.clone());

    let mut out = Vec::new();
    for (index, spec) in ephemeral.exports.iter().enumerate() {
        if !spec.when.fires_on(gate) {
            continue;
        }
        out.push(one_export_job(
            &ns,
            &name,
            &process_ref,
            uid,
            previous_root.as_deref(),
            index,
            spec,
            image,
            service_account,
        )?);
    }
    Ok(out)
}

fn one_export_job(
    ns: &str,
    name: &str,
    process_ref: &str,
    uid: &str,
    previous_root: Option<&str>,
    index: usize,
    spec: &ExportSpec,
    image: &str,
    service_account: &str,
) -> Result<Value> {
    let job_name = export_job_name(name, index);
    let receipt_cm = export_receipt_configmap_name(name, index);
    let spec_json = serde_json::to_string(spec)?;

    let mut args = vec![
        Value::from("--spec"),
        Value::from(spec_json),
        Value::from("--process-namespace"),
        Value::from(ns.to_string()),
        Value::from("--process-name"),
        Value::from(name.to_string()),
        Value::from("--receipt-configmap"),
        Value::from(receipt_cm),
    ];
    if let Some(prev) = previous_root {
        args.push(Value::from("--previous-root"));
        args.push(Value::from(prev.to_string()));
    }

    let mut owner_refs = vec![];
    if !uid.is_empty() {
        owner_refs.push(json!({
            "apiVersion": format!("{}/{}", tatara_process::GROUP, tatara_process::VERSION),
            "kind": "Process",
            "name": name,
            "uid": uid,
            "controller": true,
            "blockOwnerDeletion": true,
        }));
    }

    // Seed the outer Job's labels map through the shared substrate
    // primitive owning the 2-slot `{MANAGED_BY, PROCESS}` ownership
    // tag on the labels axis (peer to how render_flux /
    // render_aplicacao seed the annotations axis via
    // `ownership_annotations`); then extend with export-specific
    // ROLE + EXPORT_INDEX labels. Post-lift the MANAGED_BY slot
    // reads `FIELD_MANAGER` rather than the hand-coded
    // `"tatara-reconciler"` literal.
    //
    // Deliberately NOT lifted: the pod template's inner
    // `spec.template.metadata.labels` below is a 3-slot
    // `{PROCESS, ROLE, EXPORT_INDEX}` shape (no MANAGED_BY) because
    // pod-template labels feed the Job's pod-selector wiring, not
    // reconciler ownership discovery — see `ownership_labels` doc
    // for the peer-shape note.
    let mut job_labels = crate::ssapply::ownership_labels(process_ref);
    job_labels.insert(
        annotations::ROLE.to_string(),
        Value::String("export".to_string()),
    );
    job_labels.insert(
        annotations::EXPORT_INDEX.to_string(),
        Value::String(index.to_string()),
    );

    Ok(json!({
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {
            "name": job_name,
            "namespace": ns,
            "labels": Value::Object(job_labels),
            "ownerReferences": owner_refs,
        },
        "spec": {
            "backoffLimit": 1,
            "ttlSecondsAfterFinished": 3600,
            "template": {
                "metadata": {
                    "labels": {
                        annotations::PROCESS: process_ref,
                        annotations::ROLE: "export",
                        annotations::EXPORT_INDEX: index.to_string(),
                    },
                },
                "spec": {
                    "restartPolicy": "Never",
                    "serviceAccountName": service_account,
                    "containers": [{
                        "name": "worker",
                        "image": image,
                        "imagePullPolicy": "IfNotPresent",
                        "args": args,
                        "resources": {
                            "requests": { "cpu": "10m", "memory": "32Mi" },
                            "limits":   { "cpu": "200m", "memory": "128Mi" },
                        },
                    }],
                },
            },
        },
    }))
}

/// Compute the `intent_hash` pillar from canonical intent bytes.
pub fn intent_hash(bytes: &[u8]) -> String {
    hex::encode(blake3::hash(bytes).as_bytes())
}

#[cfg(test)]
mod aplicacao_tests {
    use super::*;
    use tatara_process::intent::AplicacaoIntent;

    fn demo_intent() -> AplicacaoIntent {
        AplicacaoIntent {
            chart_ref: "oci://ghcr.io/pleme-io/charts/lareira-demo-app".into(),
            version: "0.5.5".into(),
            profile: "all-in-one".into(),
            values_overlay: serde_json::json!({
                "cluster": { "name": "ephemeral-test-01" },
                "data": { "mysql": { "persistence": { "enabled": false } } },
                "compliance": { "overlays": [] }
            }),
            release_name: Some("demo-app-consolidated".into()),
            target_namespace: Some("demo-test".into()),
            install_timeout: Some("25m".into()),
        }
    }

    #[test]
    fn oci_emits_ocirepository_plus_helmrelease() {
        let (resources, intent_bytes) =
            render_aplicacao("ephemeral-demo", "demo-test", &demo_intent());
        assert_eq!(resources.len(), 2);
        assert_eq!(resources[0]["kind"], "OCIRepository");
        assert_eq!(resources[1]["kind"], "HelmRelease");
        // OCIRepository in Process's namespace + name → same as Process.
        assert_eq!(resources[0]["metadata"]["name"], "ephemeral-demo");
        assert_eq!(resources[0]["metadata"]["namespace"], "demo-test");
        assert_eq!(
            resources[0]["spec"]["url"],
            "oci://ghcr.io/pleme-io/charts/lareira-demo-app"
        );
        assert_eq!(resources[0]["spec"]["ref"]["tag"], "0.5.5");
        // HelmRelease references the OCIRepository via chartRef.
        assert_eq!(resources[1]["spec"]["chartRef"]["kind"], "OCIRepository");
        assert_eq!(resources[1]["spec"]["chartRef"]["name"], "ephemeral-demo");
        // releaseName + targetNamespace honored.
        assert_eq!(resources[1]["spec"]["releaseName"], "demo-app-consolidated");
        assert_eq!(resources[1]["spec"]["targetNamespace"], "demo-test");
        // profile injected into values (typed switch for the chart).
        assert_eq!(resources[1]["spec"]["values"]["profile"], "all-in-one");
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
        let mut a = demo_intent();
        a.target_namespace = None;
        a.release_name = None;
        let (resources, _) = render_aplicacao("test-proc", "my-ns", &a);
        let hr = resources
            .iter()
            .find(|r| r["kind"] == "HelmRelease")
            .unwrap();
        assert_eq!(hr["spec"]["targetNamespace"], "my-ns");
        assert_eq!(hr["spec"]["releaseName"], "test-proc");
    }

    #[test]
    fn install_timeout_defaults_to_25m() {
        let mut a = demo_intent();
        a.install_timeout = None;
        let (resources, _) = render_aplicacao("p", "ns", &a);
        let hr = resources
            .iter()
            .find(|r| r["kind"] == "HelmRelease")
            .unwrap();
        assert_eq!(hr["spec"]["install"]["timeout"], "25m");
        assert_eq!(hr["spec"]["install"]["remediation"]["retries"], 3);
    }

    /// Substrate-primitive pin: `HelmRelease.spec.install` and
    /// `HelmRelease.spec.upgrade` carry byte-identical policies AND
    /// match `serde_json::to_value(a.helm_lifecycle_policy())`
    /// verbatim. Pre-lift the reconciler hand-authored TWO adjacent
    /// identical `json!` blocks past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold — a regression that drifted one slot's
    /// timeout / retries away from the other, or that stopped
    /// routing through
    /// [`tatara_process::prelude::AplicacaoIntent::helm_lifecycle_policy`],
    /// would surface here rather than as an operator-observed slot
    /// asymmetry at every deployment. Sweeps both the fallback branch
    /// (`install_timeout: None` → `25m`) and the override branch
    /// (`install_timeout: Some("10m")`) so the pin binds both
    /// arms of the resolver at ONE test.
    #[test]
    fn helmrelease_install_and_upgrade_carry_byte_identical_lifecycle_policy() {
        for override_timeout in [None, Some("10m"), Some("1h30m")] {
            let mut a = demo_intent();
            a.install_timeout = override_timeout.map(str::to_string);
            let (resources, _) = render_aplicacao("p", "ns", &a);
            let hr = resources
                .iter()
                .find(|r| r["kind"] == "HelmRelease")
                .unwrap();
            let install = &hr["spec"]["install"];
            let upgrade = &hr["spec"]["upgrade"];
            assert_eq!(
                install, upgrade,
                "install/upgrade should carry identical policy"
            );
            let expected = serde_json::to_value(a.helm_lifecycle_policy()).unwrap();
            assert_eq!(
                install, &expected,
                "install slot should route through helm_lifecycle_policy verbatim",
            );
            assert_eq!(
                upgrade, &expected,
                "upgrade slot should route through helm_lifecycle_policy verbatim",
            );
        }
    }

    #[test]
    fn helmrepository_chartref_for_non_oci() {
        let a = AplicacaoIntent {
            chart_ref: "pleme-io/lareira-demo-app".into(),
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
            "lareira-demo-app"
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
        let a = demo_intent();
        let (resources, _) = render_aplicacao("ephemeral-demo", "demo-test", &a);
        for r in &resources {
            let anns = &r["metadata"]["annotations"];
            assert_eq!(
                anns[tatara_process::annotations::MANAGED_BY],
                "tatara-reconciler"
            );
            assert_eq!(
                anns[tatara_process::annotations::PROCESS],
                "demo-test/ephemeral-demo"
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
            aplicacao: Some(demo_intent()),
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
            routing: None,
            encapsulates: None,
            suspended: false,
        };
        let mut proc = Process::new("ephemeral-demo", spec);
        proc.meta_mut().namespace = Some("demo-test".into());
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

#[cfg(test)]
mod export_job_tests {
    use super::*;
    use tatara_process::attestation::ProcessAttestation;
    use tatara_process::classification::{Classification, ConvergencePointType, SubstrateType};
    use tatara_process::crd::{ProcessSpec, ProcessStatus};
    use tatara_process::export::{
        ArtifactSource, ExportSpec, ExportTrigger, HttpEventChannel, NatsSubjectChannel,
        ReceiptsSource, RunMarkerSource, VectorChannel,
    };
    use tatara_process::lifetime::{EphemeralLifetime, Lifetime, TeardownPolicy};

    fn spec_receipts_attested() -> ExportSpec {
        ExportSpec {
            source: ArtifactSource {
                receipts: Some(ReceiptsSource::default()),
                ..ArtifactSource::default()
            },
            channel: VectorChannel {
                nats_subject: Some(NatsSubjectChannel {
                    subject: "pleme.pleme-dev.ephemeral.{{run_id}}.receipt".into(),
                    stream: "EPHEMERAL_RECEIPTS".into(),
                    url: None,
                }),
                ..VectorChannel::default()
            },
            when: ExportTrigger::OnAttested,
            experiment_id_override: None,
        }
    }

    fn spec_run_marker_always() -> ExportSpec {
        ExportSpec {
            source: ArtifactSource {
                run_marker: Some(RunMarkerSource::default()),
                ..ArtifactSource::default()
            },
            channel: VectorChannel {
                http_event: Some(HttpEventChannel {
                    endpoint: None,
                    signal_type: "ephemeral-marker".into(),
                }),
                ..VectorChannel::default()
            },
            when: ExportTrigger::Always,
            experiment_id_override: None,
        }
    }

    fn process_with(exports: Vec<ExportSpec>, with_prev_root: bool) -> Process {
        let mut status = ProcessStatus::default();
        if with_prev_root {
            status.attestation = Some(ProcessAttestation::initial(
                "art".into(),
                None,
                "intent".into(),
            ));
        }
        let spec = ProcessSpec {
            identity: Default::default(),
            classification: Classification {
                point_type: ConvergencePointType::Gate,
                substrate: SubstrateType::Compute,
                horizon: Default::default(),
                calm: Default::default(),
                data_classification: Default::default(),
            },
            intent: Default::default(),
            boundary: Default::default(),
            compliance: Default::default(),
            depends_on: vec![],
            signals: Default::default(),
            lifetime: Lifetime {
                ephemeral: Some(EphemeralLifetime {
                    ttl: "1h".into(),
                    teardown_policy: TeardownPolicy::OnAttested,
                    max_concurrent: 1,
                    exports,
                }),
                ..Lifetime::default()
            },
            routing: None,
            encapsulates: None,
            suspended: false,
        };
        let mut p = Process::new("r1", spec);
        p.metadata.namespace = Some("demo-test".into());
        p.metadata.uid = Some("uid-abc".into());
        p.status = Some(status);
        p
    }

    #[test]
    fn no_exports_no_jobs() {
        let p = process_with(vec![], false);
        let jobs = render_export_jobs(
            &p,
            ProcessPhase::Attested,
            "ghcr.io/x/worker:0",
            "tatara-export-worker",
        )
        .unwrap();
        assert!(jobs.is_empty());
    }

    #[test]
    fn renders_one_job_per_applicable_export() {
        let p = process_with(
            vec![spec_receipts_attested(), spec_run_marker_always()],
            false,
        );
        // Both fire on Attested → 2 jobs.
        let jobs = render_export_jobs(
            &p,
            ProcessPhase::Attested,
            "ghcr.io/pleme-io/tatara-export-worker:0.2.0",
            "tatara-export-worker",
        )
        .unwrap();
        assert_eq!(jobs.len(), 2);

        // Only the Always one fires on Failed → 1 job.
        let jobs = render_export_jobs(
            &p,
            ProcessPhase::Failed,
            "ghcr.io/pleme-io/tatara-export-worker:0.2.0",
            "tatara-export-worker",
        )
        .unwrap();
        assert_eq!(jobs.len(), 1);
    }

    #[test]
    fn rendered_job_carries_canonical_labels() {
        let p = process_with(vec![spec_receipts_attested()], false);
        let jobs = render_export_jobs(
            &p,
            ProcessPhase::Attested,
            "img:tag",
            "tatara-export-worker",
        )
        .unwrap();
        let labels = &jobs[0]["metadata"]["labels"];
        assert_eq!(labels[tatara_process::annotations::PROCESS], "demo-test/r1");
        assert_eq!(labels[tatara_process::annotations::ROLE], "export");
        assert_eq!(labels[tatara_process::annotations::EXPORT_INDEX], "0");
        assert_eq!(jobs[0]["metadata"]["name"], "r1-export-0");
        assert_eq!(jobs[0]["metadata"]["namespace"], "demo-test");
    }

    #[test]
    fn rendered_job_has_owner_reference_to_process() {
        let p = process_with(vec![spec_receipts_attested()], false);
        let jobs = render_export_jobs(&p, ProcessPhase::Attested, "img", "sa").unwrap();
        let owner = &jobs[0]["metadata"]["ownerReferences"][0];
        assert_eq!(owner["kind"], "Process");
        assert_eq!(owner["name"], "r1");
        assert_eq!(owner["uid"], "uid-abc");
        assert_eq!(owner["controller"], true);
        assert_eq!(owner["blockOwnerDeletion"], true);
    }

    #[test]
    fn rendered_job_passes_spec_as_argv() {
        let p = process_with(vec![spec_receipts_attested()], false);
        let jobs = render_export_jobs(&p, ProcessPhase::Attested, "img", "sa").unwrap();
        let args = jobs[0]["spec"]["template"]["spec"]["containers"][0]["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap_or(""))
            .collect::<Vec<_>>();
        // --spec contains the serialized ExportSpec JSON.
        let i_spec = args.iter().position(|a| *a == "--spec").unwrap();
        let spec_arg: ExportSpec = serde_json::from_str(args[i_spec + 1]).unwrap();
        assert!(spec_arg.source.receipts.is_some());

        // Downward-API stamps for the worker.
        let i_ns = args
            .iter()
            .position(|a| *a == "--process-namespace")
            .unwrap();
        assert_eq!(args[i_ns + 1], "demo-test");
        let i_n = args.iter().position(|a| *a == "--process-name").unwrap();
        assert_eq!(args[i_n + 1], "r1");
        let i_rcm = args
            .iter()
            .position(|a| *a == "--receipt-configmap")
            .unwrap();
        assert_eq!(args[i_rcm + 1], "r1-export-0-receipt");
    }

    #[test]
    fn rendered_job_includes_previous_root_when_attestation_present() {
        let p_no_root = process_with(vec![spec_receipts_attested()], false);
        let p_with_root = process_with(vec![spec_receipts_attested()], true);

        let j_no = render_export_jobs(&p_no_root, ProcessPhase::Attested, "img", "sa").unwrap();
        let j_with = render_export_jobs(&p_with_root, ProcessPhase::Attested, "img", "sa").unwrap();

        let args_no = j_no[0]["spec"]["template"]["spec"]["containers"][0]["args"]
            .as_array()
            .unwrap();
        let args_with = j_with[0]["spec"]["template"]["spec"]["containers"][0]["args"]
            .as_array()
            .unwrap();
        assert!(args_no
            .iter()
            .all(|v| v.as_str() != Some("--previous-root")));
        assert!(args_with
            .iter()
            .any(|v| v.as_str() == Some("--previous-root")));
    }

    #[test]
    fn rendered_job_uses_supplied_image_and_service_account() {
        let p = process_with(vec![spec_receipts_attested()], false);
        let jobs = render_export_jobs(
            &p,
            ProcessPhase::Attested,
            "ghcr.io/pleme-io/tatara-export-worker:0.2.0",
            "custom-sa",
        )
        .unwrap();
        assert_eq!(
            jobs[0]["spec"]["template"]["spec"]["containers"][0]["image"],
            "ghcr.io/pleme-io/tatara-export-worker:0.2.0"
        );
        assert_eq!(
            jobs[0]["spec"]["template"]["spec"]["serviceAccountName"],
            "custom-sa"
        );
        assert_eq!(
            jobs[0]["spec"]["template"]["spec"]["restartPolicy"],
            "Never"
        );
        assert_eq!(jobs[0]["spec"]["backoffLimit"], 1);
        assert_eq!(jobs[0]["spec"]["ttlSecondsAfterFinished"], 3600);
    }

    #[test]
    fn export_job_name_is_deterministic() {
        assert_eq!(export_job_name("r1", 0), "r1-export-0");
        assert_eq!(export_job_name("attest", 5), "attest-export-5");
    }

    #[test]
    fn export_receipt_configmap_name_is_deterministic() {
        assert_eq!(
            export_receipt_configmap_name("r1", 0),
            "r1-export-0-receipt"
        );
    }

    #[test]
    fn export_receipt_configmap_name_composes_through_substrate_primitive() {
        // Path-uniformity pin: the export-worker's receipt-CM composer
        // must compose through the substrate's ONE default-derivation
        // owner (`tatara_process::receipt::default_receipt_config_map_name`
        // + the `RECEIPT_CM_SUFFIX` const) rather than re-inline the
        // `-receipt` suffix at this crate. A regression that
        // re-inlined the shape as `format!("{name}-export-{i}-receipt")`
        // would silently drift the moment the substrate's suffix
        // changed (e.g. to `-attest` or `.receipt`), because this
        // crate's byte would no longer route through the const. The
        // pin re-reads both the Job-name composer (`export_job_name`)
        // AND the substrate composer at test time, so the equality
        // holds iff both live paths are the current implementation.
        for (process_name, index) in [("r1", 0usize), ("attest", 5), ("svc-abc-def", 12)] {
            let via_composer = export_receipt_configmap_name(process_name, index);
            let via_substrate = tatara_process::receipt::default_receipt_config_map_name(
                &export_job_name(process_name, index),
            );
            assert_eq!(
                via_composer, via_substrate,
                "export_receipt_configmap_name({process_name:?}, {index}) drifted from \
                 <export_job_name>++substrate::default_receipt_config_map_name composition"
            );
        }
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    use std::collections::BTreeMap;
    use tatara_process::classification::{Classification, ConvergencePointType, SubstrateType};
    use tatara_process::crd::ProcessSpec;
    use tatara_process::routing::{RoutingBackend, RoutingHostname, RoutingSpec};

    fn demo_process(routing: Option<RoutingSpec>) -> Process {
        let spec = ProcessSpec {
            identity: Default::default(),
            classification: Classification {
                point_type: ConvergencePointType::Gate,
                substrate: SubstrateType::Compute,
                horizon: Default::default(),
                calm: Default::default(),
                data_classification: Default::default(),
            },
            intent: Default::default(),
            boundary: Default::default(),
            compliance: Default::default(),
            depends_on: vec![],
            signals: Default::default(),
            lifetime: Default::default(),
            routing,
            encapsulates: None,
            suspended: false,
        };
        let mut p = Process::new("demo-prod", spec);
        p.metadata.namespace = Some("demo-ns".into());
        p.metadata.uid = Some("uid-1".into());
        p
    }

    fn two_hostname_routing(stable: bool) -> RoutingSpec {
        RoutingSpec {
            hostnames: vec![
                RoutingHostname {
                    app: "api".into(),
                    instance: Some("demo-prod".into()),
                    cluster: None,
                },
                RoutingHostname {
                    app: "gateway".into(),
                    instance: Some("demo-prod".into()),
                    cluster: None,
                },
            ],
            backend: RoutingBackend {
                service: "demo-app-gateway".into(),
                port: 8000,
                tls_issuer: None,
                ingress_annotations: BTreeMap::new(),
            },
            stable_name_claim: stable,
            priority: 100,
        }
    }

    #[test]
    fn emits_ingress_plus_dns_per_hostname() {
        let r = two_hostname_routing(false);
        let p = demo_process(Some(r.clone()));
        let out = render_routing(
            &p,
            &r,
            false,
            "pleme-dev",
            "use1",
            "quero.lol",
            Some("pleme-dev.use1.quero.lol"),
        )
        .unwrap();
        // 2 hostnames × 2 edges = 4 resources.
        assert_eq!(out.len(), 4);
        let kinds: Vec<_> = out.iter().map(|v| v["kind"].as_str().unwrap()).collect();
        assert!(kinds.contains(&"Ingress"));
        assert!(kinds.contains(&"DNSEndpoint"));
    }

    #[test]
    fn stable_claim_doubles_emission() {
        let r = two_hostname_routing(true);
        let p = demo_process(Some(r.clone()));
        // Without holding the claim → 4 resources (per-instance only).
        let without = render_routing(
            &p,
            &r,
            false,
            "pleme-dev",
            "use1",
            "quero.lol",
            Some("pleme-dev.use1.quero.lol"),
        )
        .unwrap();
        assert_eq!(without.len(), 4);

        // Holding the claim → 8 resources (per-instance + stable).
        let with = render_routing(
            &p,
            &r,
            true,
            "pleme-dev",
            "use1",
            "quero.lol",
            Some("pleme-dev.use1.quero.lol"),
        )
        .unwrap();
        assert_eq!(with.len(), 8);
        let stable_count = with
            .iter()
            .filter(|v| {
                v["metadata"]["annotations"]["tatara.pleme.io/routing-form"] == "stable"
                    || v["metadata"]["labels"]["tatara.pleme.io/routing-form"] == "stable"
            })
            .count();
        assert_eq!(stable_count, 4); // 2 hostnames × 2 edges in stable form
    }

    #[test]
    fn omits_dns_when_lb_target_absent() {
        let r = two_hostname_routing(false);
        let p = demo_process(Some(r.clone()));
        let out = render_routing(&p, &r, false, "pleme-dev", "use1", "quero.lol", None).unwrap();
        // 2 hostnames × 1 edge (Ingress only — DNSEndpoint skipped) = 2.
        assert_eq!(out.len(), 2);
        for v in &out {
            assert_eq!(v["kind"], "Ingress");
        }
    }

    #[test]
    fn empty_hostnames_emits_nothing() {
        let r = RoutingSpec {
            hostnames: vec![],
            backend: RoutingBackend {
                service: "svc".into(),
                port: 80,
                tls_issuer: None,
                ingress_annotations: BTreeMap::new(),
            },
            stable_name_claim: false,
            priority: 0,
        };
        let p = demo_process(Some(r.clone()));
        let out = render_routing(&p, &r, false, "pleme-dev", "use1", "quero.lol", None).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn anonymous_hostname_uses_content_hash() {
        let r = RoutingSpec {
            hostnames: vec![RoutingHostname {
                app: "smoke".into(),
                instance: None, // ⇒ content-hash form
                cluster: None,
            }],
            backend: RoutingBackend {
                service: "svc".into(),
                port: 80,
                tls_issuer: None,
                ingress_annotations: BTreeMap::new(),
            },
            stable_name_claim: false,
            priority: 0,
        };
        let p = demo_process(Some(r.clone()));
        let out = render_routing(&p, &r, false, "pleme-dev", "use1", "quero.lol", None).unwrap();
        let host = out[0]["spec"]["rules"][0]["host"].as_str().unwrap();
        // Shape: smoke.<8-hex>.pleme-dev.use1.quero.lol
        assert!(host.starts_with("smoke."));
        assert!(host.ends_with(".pleme-dev.use1.quero.lol"));
        let middle: Vec<_> = host.split('.').collect();
        assert_eq!(middle[1].len(), 8); // BLAKE3:8 hex
        assert!(middle[1].chars().all(|c| c.is_ascii_hexdigit()));
    }
}
