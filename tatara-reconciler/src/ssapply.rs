//! DynamicObject server-side apply — the bridge between the reconciler
//! and arbitrary K8s resources emitted by `render`.
//!
//! Pure helpers (owner ref injection, plural lookup, Ready condition parsing)
//! are unit-tested; the `apply_owned`/`fetch` entry points require a cluster.

use anyhow::{anyhow, Result};
use kube::api::{ApiResource, DynamicObject, Patch, PatchParams};
use kube::{Api, Client};
use serde_json::{json, Value};

use tatara_process::annotations;
use tatara_process::prelude::Process;

/// Field manager string we use for all SSA writes.
pub const FIELD_MANAGER: &str = "tatara-reconciler";

/// Resolve an `ApiResource` for `apiVersion/kind`. Hand-maintains plurals
/// for resources we emit or consume — good enough for v0; future move to
/// `kube::discovery` lands when we want to handle arbitrary CRDs.
pub fn api_resource(api_version: &str, kind: &str) -> Result<ApiResource> {
    let (group, version) = match api_version.split_once('/') {
        Some((g, v)) => (g.to_string(), v.to_string()),
        // Core/v1 has no group — api_version is just "v1".
        None => (String::new(), api_version.to_string()),
    };
    let plural = plural_of(kind)?;
    Ok(ApiResource {
        group,
        version,
        api_version: api_version.to_string(),
        kind: kind.to_string(),
        plural: plural.to_string(),
    })
}

fn plural_of(kind: &str) -> Result<&'static str> {
    match kind {
        // Flux source-controller
        "GitRepository" => Ok("gitrepositories"),
        "HelmRepository" => Ok("helmrepositories"),
        "OCIRepository" => Ok("ocirepositories"),
        "Bucket" => Ok("buckets"),
        // Flux kustomize-controller
        "Kustomization" => Ok("kustomizations"),
        // Flux helm-controller
        "HelmRelease" => Ok("helmreleases"),
        // Core kinds we might emit later
        "ConfigMap" => Ok("configmaps"),
        "Secret" => Ok("secrets"),
        "Namespace" => Ok("namespaces"),
        other => Err(anyhow!("unknown plural for kind {other:?}")),
    }
}

/// Server-side apply a JSON resource, injecting owner reference + standard
/// tatara annotations derived from the Process.
pub async fn apply_owned(
    client: Client,
    process: &Process,
    namespace: &str,
    mut resource: Value,
) -> Result<()> {
    inject_owner_reference(&mut resource, build_owner_reference(process)?)?;
    inject_annotations(&mut resource, process)?;

    let api_version = resource
        .get("apiVersion")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("resource missing apiVersion"))?
        .to_string();
    let kind = resource
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("resource missing kind"))?
        .to_string();
    let name = resource
        .get("metadata")
        .and_then(|m| m.get("name"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("resource missing metadata.name"))?
        .to_string();

    let ar = api_resource(&api_version, &kind)?;
    let obj: DynamicObject = serde_json::from_value(resource)?;
    let api: Api<DynamicObject> = Api::namespaced_with(client, namespace, &ar);

    let pp = PatchParams::apply(FIELD_MANAGER).force();
    api.patch(&name, &pp, &Patch::Apply(&obj))
        .await
        .map_err(|e| anyhow!("ssapply {kind}/{name}: {e}"))?;
    Ok(())
}

/// Fetch a DynamicObject by kind + namespace + name. Returns None on 404.
pub async fn fetch(
    client: Client,
    namespace: &str,
    api_version: &str,
    kind: &str,
    name: &str,
) -> Result<Option<DynamicObject>> {
    let ar = api_resource(api_version, kind)?;
    let api: Api<DynamicObject> = Api::namespaced_with(client, namespace, &ar);
    Ok(api.get_opt(name).await?)
}

/// Parsed readiness state of a resource's `status.conditions[type=Ready]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadyState {
    Ready,
    NotReady(Option<String>),
    Unknown,
}

/// Extract `status.conditions[type=Ready]` from a DynamicObject.
pub fn ready_condition(obj: &DynamicObject) -> ReadyState {
    ready_condition_value(&obj.data)
}

/// Same extraction but operating on raw JSON — testable without a cluster.
pub fn ready_condition_value(data: &Value) -> ReadyState {
    let conditions = data
        .get("status")
        .and_then(|s| s.get("conditions"))
        .and_then(|c| c.as_array());
    let Some(conditions) = conditions else {
        return ReadyState::Unknown;
    };
    for c in conditions {
        let Some(typ) = c.get("type").and_then(|v| v.as_str()) else {
            continue;
        };
        if typ != "Ready" {
            continue;
        }
        return match c.get("status").and_then(|v| v.as_str()) {
            Some("True") => ReadyState::Ready,
            Some("False") => ReadyState::NotReady(
                c.get("message").and_then(|v| v.as_str()).map(String::from),
            ),
            _ => ReadyState::Unknown,
        };
    }
    ReadyState::Unknown
}

fn build_owner_reference(p: &Process) -> Result<Value> {
    let name = p
        .metadata
        .name
        .clone()
        .ok_or_else(|| anyhow!("process missing metadata.name"))?;
    let uid = p
        .metadata
        .uid
        .clone()
        .ok_or_else(|| anyhow!("process missing metadata.uid"))?;
    Ok(json!({
        "apiVersion": "tatara.pleme.io/v1alpha1",
        "kind": "Process",
        "name": name,
        "uid": uid,
        "controller": true,
        "blockOwnerDeletion": true,
    }))
}

fn inject_owner_reference(resource: &mut Value, owner_ref: Value) -> Result<()> {
    let metadata = resource
        .as_object_mut()
        .ok_or_else(|| anyhow!("resource is not an object"))?
        .entry("metadata")
        .or_insert_with(|| Value::Object(Default::default()));
    let md = metadata
        .as_object_mut()
        .ok_or_else(|| anyhow!("metadata is not an object"))?;
    let refs = md
        .entry("ownerReferences")
        .or_insert_with(|| Value::Array(vec![]));
    if let Value::Array(arr) = refs {
        arr.push(owner_ref);
    }
    Ok(())
}

fn inject_annotations(resource: &mut Value, process: &Process) -> Result<()> {
    let metadata = resource
        .as_object_mut()
        .ok_or_else(|| anyhow!("resource is not an object"))?
        .entry("metadata")
        .or_insert_with(|| Value::Object(Default::default()));
    let md = metadata
        .as_object_mut()
        .ok_or_else(|| anyhow!("metadata is not an object"))?;
    let annot = md
        .entry("annotations")
        .or_insert_with(|| Value::Object(Default::default()));
    let annot = annot
        .as_object_mut()
        .ok_or_else(|| anyhow!("annotations is not an object"))?;

    let ns = process.metadata.namespace.as_deref().unwrap_or("default");
    let name = process.metadata.name.as_deref().unwrap_or("unnamed");
    annot.insert(
        annotations::MANAGED_BY.to_string(),
        Value::String(FIELD_MANAGER.to_string()),
    );
    annot.insert(
        annotations::PROCESS.to_string(),
        Value::String(format!("{ns}/{name}")),
    );

    if let Some(status) = &process.status {
        if let Some(pid) = &status.pid {
            annot.insert(annotations::PID.to_string(), Value::String(pid.clone()));
        }
        if let Some(id) = &status.identity {
            annot.insert(
                annotations::CONTENT_HASH.to_string(),
                Value::String(id.content_hash.clone()),
            );
        }
        if let Some(a) = &status.attestation {
            annot.insert(
                annotations::GENERATION.to_string(),
                Value::String(a.generation.to_string()),
            );
            annot.insert(
                annotations::ATTESTATION_ROOT.to_string(),
                Value::String(a.composed_root.clone()),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plurals_resolve() {
        assert_eq!(plural_of("Kustomization").unwrap(), "kustomizations");
        assert_eq!(plural_of("HelmRelease").unwrap(), "helmreleases");
        assert_eq!(plural_of("GitRepository").unwrap(), "gitrepositories");
        assert!(plural_of("Nonsense").is_err());
    }

    #[test]
    fn api_resource_splits_group_and_version() {
        let ar = api_resource("kustomize.toolkit.fluxcd.io/v1", "Kustomization").unwrap();
        assert_eq!(ar.group, "kustomize.toolkit.fluxcd.io");
        assert_eq!(ar.version, "v1");
        assert_eq!(ar.plural, "kustomizations");
    }

    #[test]
    fn api_resource_handles_core_v1() {
        let ar = api_resource("v1", "ConfigMap").unwrap();
        assert_eq!(ar.group, "");
        assert_eq!(ar.version, "v1");
        assert_eq!(ar.plural, "configmaps");
    }

    #[test]
    fn ready_condition_true() {
        let data = json!({
            "status": { "conditions": [
                { "type": "Ready", "status": "True" }
            ]}
        });
        assert_eq!(ready_condition_value(&data), ReadyState::Ready);
    }

    #[test]
    fn ready_condition_false_with_message() {
        let data = json!({
            "status": { "conditions": [
                { "type": "Ready", "status": "False", "message": "pull failed" }
            ]}
        });
        assert_eq!(
            ready_condition_value(&data),
            ReadyState::NotReady(Some("pull failed".to_string()))
        );
    }

    #[test]
    fn ready_condition_missing_is_unknown() {
        let data = json!({ "status": { "conditions": [] } });
        assert_eq!(ready_condition_value(&data), ReadyState::Unknown);
        let data = json!({});
        assert_eq!(ready_condition_value(&data), ReadyState::Unknown);
    }

    #[test]
    fn inject_owner_reference_adds_entry() {
        let mut obj = json!({
            "apiVersion": "v1", "kind": "ConfigMap",
            "metadata": { "name": "x" },
        });
        inject_owner_reference(
            &mut obj,
            json!({ "apiVersion": "tatara.pleme.io/v1alpha1", "kind": "Process", "name": "p", "uid": "u" }),
        )
        .unwrap();
        let refs = obj["metadata"]["ownerReferences"].as_array().unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0]["kind"], "Process");
    }
}
