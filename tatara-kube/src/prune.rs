use crate::error::KubeError;
use crate::ordering;
use crate::resource::{ResourceIdentity, LABEL_CLUSTER, LABEL_GENERATION, LABEL_MANAGED_BY, LABEL_MANAGED_BY_VALUE};
use kube::{
    api::{Api, ApiResource, DeleteParams, DynamicObject, ListParams},
    discovery::{self, Scope},
    Client,
};
use tracing::{debug, info, warn};

/// Find all resources managed by tatara-kube for a given cluster
/// whose generation hash does not match the current desired state.
pub async fn find_orphans(
    client: &Client,
    cluster_name: &str,
    current_generation: &str,
) -> Result<Vec<(ResourceIdentity, ApiResource, Scope)>, KubeError> {
    let label_selector = format!(
        "{}={},{}={}",
        LABEL_MANAGED_BY, LABEL_MANAGED_BY_VALUE, LABEL_CLUSTER, cluster_name,
    );

    let mut orphans = Vec::new();

    let discovery = discovery::Discovery::new(client.clone())
        .run()
        .await
        .map_err(|e| KubeError::Other(e.into()))?;

    for group in discovery.groups() {
        for (ar, caps) in group.recommended_resources() {
            let api: Api<DynamicObject> = Api::all_with(client.clone(), &ar);

            let list = match api
                .list(&ListParams::default().labels(&label_selector))
                .await
            {
                Ok(list) => list,
                Err(_) => continue, // skip resources we can't list
            };

            for obj in list.items {
                let gen = obj
                    .metadata
                    .labels
                    .as_ref()
                    .and_then(|l| l.get(LABEL_GENERATION))
                    .map(|s| s.as_str())
                    .unwrap_or("");

                if gen != current_generation {
                    let identity = ResourceIdentity {
                        api_version: obj
                            .types
                            .as_ref()
                            .map(|t| t.api_version.clone())
                            .unwrap_or_default(),
                        kind: obj
                            .types
                            .as_ref()
                            .map(|t| t.kind.clone())
                            .unwrap_or_default(),
                        namespace: obj.metadata.namespace.clone(),
                        name: obj.metadata.name.unwrap_or_default(),
                    };
                    debug!(resource = %identity, "found orphan");
                    orphans.push((identity, ar.clone(), caps.scope.clone()));
                }
            }
        }
    }

    // Sort orphans in reverse dependency order for safe deletion
    orphans.sort_by(|a, b| {
        let tier_a = ordering::kind_priority_for_prune(&a.0.kind);
        let tier_b = ordering::kind_priority_for_prune(&b.0.kind);
        tier_b.cmp(&tier_a)
    });

    Ok(orphans)
}

/// Delete orphaned resources.
pub async fn prune_orphans(
    client: &Client,
    orphans: &[(ResourceIdentity, ApiResource, Scope)],
) -> Vec<Result<ResourceIdentity, KubeError>> {
    let mut results = Vec::new();

    for (identity, ar, scope) in orphans {
        let api: Api<DynamicObject> = match (&identity.namespace, scope) {
            (Some(ns), Scope::Namespaced) => Api::namespaced_with(client.clone(), ns, ar),
            _ => Api::all_with(client.clone(), ar),
        };

        info!(resource = %identity, "pruning orphaned resource");

        match api.delete(&identity.name, &DeleteParams::default()).await {
            Ok(_) => results.push(Ok(identity.clone())),
            Err(e) => {
                warn!(resource = %identity, error = %e, "failed to prune");
                results.push(Err(KubeError::PruneFailed {
                    kind: identity.kind.clone(),
                    name: identity.name.clone(),
                    reason: e.to_string(),
                }));
            }
        }
    }

    results
}
