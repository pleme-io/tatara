use crate::error::KubeError;
use crate::resource::ManagedResource;
use kube::{
    api::{Api, DynamicObject, Patch, PatchParams},
    discovery::{self, Scope},
    Client,
};
use tracing::{debug, info};

/// Extract `GroupVersionKind` from a manifest JSON value.
fn extract_gvk(manifest: &serde_json::Value) -> Result<kube::api::GroupVersionKind, KubeError> {
    let api_version =
        manifest["apiVersion"]
            .as_str()
            .ok_or_else(|| KubeError::ResourceParseFailed {
                reason: "missing apiVersion".to_string(),
            })?;
    let kind = manifest["kind"]
        .as_str()
        .ok_or_else(|| KubeError::ResourceParseFailed {
            reason: "missing kind".to_string(),
        })?;

    let (group, version) = if let Some(idx) = api_version.rfind('/') {
        (&api_version[..idx], &api_version[idx + 1..])
    } else {
        ("", api_version)
    };

    Ok(kube::api::GroupVersionKind::gvk(group, version, kind))
}

/// Apply a single resource using Kubernetes Server-Side Apply.
///
/// Uses `DynamicObject` and API discovery to handle any resource type
/// including CRDs. The field manager identifies tatara-kube as the owner.
pub async fn server_side_apply(
    client: &Client,
    resource: &ManagedResource,
    field_manager: &str,
    force: bool,
) -> Result<DynamicObject, KubeError> {
    let gvk = extract_gvk(&resource.manifest)?;

    debug!(
        kind = %resource.identity.kind,
        name = %resource.identity.name,
        "discovering API resource"
    );

    let (ar, caps) =
        discovery::pinned_kind(client, &gvk)
            .await
            .map_err(|e| KubeError::DiscoveryFailed {
                api_version: resource.identity.api_version.clone(),
                kind: resource.identity.kind.clone(),
                reason: e.to_string(),
            })?;

    let api: Api<DynamicObject> = if caps.scope == Scope::Namespaced {
        let ns = resource.identity.namespace.as_deref().unwrap_or("default");
        Api::namespaced_with(client.clone(), ns, &ar)
    } else {
        Api::all_with(client.clone(), &ar)
    };

    let mut pp = PatchParams::apply(field_manager);
    if force {
        pp = pp.force();
    }

    let obj: DynamicObject =
        serde_json::from_value(resource.manifest.clone()).map_err(|e| KubeError::ApplyFailed {
            kind: resource.identity.kind.clone(),
            name: resource.identity.name.clone(),
            reason: e.to_string(),
        })?;

    info!(
        kind = %resource.identity.kind,
        name = %resource.identity.name,
        namespace = ?resource.identity.namespace,
        "applying resource via SSA"
    );

    let result = api
        .patch(&resource.identity.name, &pp, &Patch::Apply(&obj))
        .await
        .map_err(|e| KubeError::ApplyFailed {
            kind: resource.identity.kind.clone(),
            name: resource.identity.name.clone(),
            reason: e.to_string(),
        })?;

    Ok(result)
}
