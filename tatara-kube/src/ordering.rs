use crate::resource::ManagedResource;

/// Priority tiers for Kubernetes resource kinds.
/// Lower number = applied first, deleted last.
fn kind_priority(kind: &str) -> u32 {
    match kind {
        "Namespace" => 0,
        "CustomResourceDefinition" => 10,
        "PriorityClass" | "StorageClass" => 15,
        "ClusterRole" | "ClusterRoleBinding" => 20,
        "ServiceAccount" | "Role" | "RoleBinding" => 30,
        "ConfigMap" | "Secret" => 40,
        "ExternalSecret" => 42,
        "PersistentVolume" => 44,
        "PersistentVolumeClaim" => 45,
        "LimitRange" | "ResourceQuota" => 50,
        "NetworkPolicy" => 55,
        "Service" => 60,
        "DatabaseMigration" => 65,
        "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet" => 70,
        "Job" | "CronJob" => 75,
        "HorizontalPodAutoscaler" | "PodDisruptionBudget" => 80,
        "ScaledObject" => 82,
        "ServiceMonitor" | "PodMonitor" | "PrometheusRule" => 90,
        "PeerAuthentication" | "DestinationRule" => 95,
        "MutatingWebhookConfiguration" | "ValidatingWebhookConfiguration" => 98,
        _ => 100,
    }
}

/// Get the effective tier for a resource, allowing annotation overrides.
fn effective_tier(resource: &ManagedResource) -> u32 {
    // Allow Nix authors to override tier with an annotation
    if let Some(tier_str) = resource
        .manifest
        .pointer("/metadata/annotations/tatara.pleme.io~1tier")
        .and_then(|v| v.as_str())
    {
        if let Ok(tier) = tier_str.parse::<u32>() {
            return tier;
        }
    }
    kind_priority(&resource.identity.kind)
}

/// Sort resources by dependency tier, then by namespace, then by name.
/// Returns resources in apply order (lowest tier first).
pub fn topological_sort(resources: &mut [ManagedResource]) {
    resources.sort_by(|a, b| {
        let tier_a = effective_tier(a);
        let tier_b = effective_tier(b);
        tier_a
            .cmp(&tier_b)
            .then_with(|| a.identity.namespace.cmp(&b.identity.namespace))
            .then_with(|| a.identity.name.cmp(&b.identity.name))
    });
}

/// Get the kind priority for use in reverse-order deletion.
pub fn kind_priority_for_prune(kind: &str) -> u32 {
    kind_priority(kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::ResourceIdentity;

    fn make_resource(kind: &str, name: &str) -> ManagedResource {
        let manifest = serde_json::json!({
            "apiVersion": "v1",
            "kind": kind,
            "metadata": { "name": name, "namespace": "default" }
        });
        ManagedResource {
            identity: ResourceIdentity {
                api_version: "v1".to_string(),
                kind: kind.to_string(),
                namespace: Some("default".to_string()),
                name: name.to_string(),
            },
            content_hash: String::new(),
            manifest,
        }
    }

    #[test]
    fn test_ordering() {
        let mut resources = vec![
            make_resource("Deployment", "app"),
            make_resource("Namespace", "default"),
            make_resource("Service", "svc"),
            make_resource("ConfigMap", "cfg"),
            make_resource("ServiceAccount", "sa"),
        ];
        topological_sort(&mut resources);
        let kinds: Vec<&str> = resources.iter().map(|r| r.identity.kind.as_str()).collect();
        assert_eq!(
            kinds,
            vec!["Namespace", "ServiceAccount", "ConfigMap", "Service", "Deployment"]
        );
    }
}
