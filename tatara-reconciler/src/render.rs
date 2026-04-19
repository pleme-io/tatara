//! Render — `Intent` → FluxCD CRs (JSON `Value` form; the controller will
//! wrap with owner references and apply via `Api<DynamicObject>`).

use anyhow::Result;
use serde_json::{json, Value};

use tatara_process::annotations;
use tatara_process::intent::{FluxIntent, Intent, IntentVariant, LispIntent, NixIntent};
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
