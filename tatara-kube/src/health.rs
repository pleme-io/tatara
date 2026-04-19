use crate::error::KubeError;
use crate::resource::ResourceIdentity;
use kube::{
    api::{Api, ApiResource, DynamicObject},
    discovery::Scope,
    Client,
};
use std::time::Duration;
use tracing::debug;

/// Resource health status.
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Progressing,
    Degraded { message: String },
    Unknown,
}

/// Wait for a resource to become healthy, polling at intervals.
pub async fn wait_healthy(
    client: &Client,
    identity: &ResourceIdentity,
    ar: &ApiResource,
    scope: Scope,
    timeout: Duration,
) -> Result<HealthStatus, KubeError> {
    let deadline = tokio::time::Instant::now() + timeout;
    let poll_interval = Duration::from_secs(2);

    loop {
        let status = check_health(client, identity, ar, scope.clone()).await?;

        match &status {
            HealthStatus::Healthy => return Ok(status),
            HealthStatus::Degraded { .. } => {
                return Err(KubeError::HealthCheckTimeout {
                    kind: identity.kind.clone(),
                    name: identity.name.clone(),
                    timeout_secs: timeout.as_secs(),
                });
            }
            HealthStatus::Progressing | HealthStatus::Unknown => {}
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(KubeError::HealthCheckTimeout {
                kind: identity.kind.clone(),
                name: identity.name.clone(),
                timeout_secs: timeout.as_secs(),
            });
        }

        tokio::time::sleep(poll_interval).await;
    }
}

/// Check health of a single resource (point-in-time).
async fn check_health(
    client: &Client,
    identity: &ResourceIdentity,
    ar: &ApiResource,
    scope: Scope,
) -> Result<HealthStatus, KubeError> {
    let api: Api<DynamicObject> = match (&identity.namespace, scope) {
        (Some(ns), Scope::Namespaced) => Api::namespaced_with(client.clone(), ns, ar),
        _ => Api::all_with(client.clone(), ar),
    };

    let obj = api.get(&identity.name).await?;
    let status = obj.data.get("status");

    debug!(kind = %identity.kind, name = %identity.name, "checking health");

    match identity.kind.as_str() {
        "Deployment" => check_deployment_health(status),
        "StatefulSet" => check_statefulset_health(status),
        "DaemonSet" => check_daemonset_health(status),
        "Job" => check_job_health(status),
        "PersistentVolumeClaim" => check_pvc_health(status),
        "CustomResourceDefinition" => check_crd_health(status),
        // Services, ConfigMaps, Secrets are healthy once created
        "Service" | "ConfigMap" | "Secret" | "ServiceAccount" | "Namespace" => {
            Ok(HealthStatus::Healthy)
        }
        _ => check_generic_conditions(status),
    }
}

fn check_deployment_health(status: Option<&serde_json::Value>) -> Result<HealthStatus, KubeError> {
    let Some(status) = status else {
        return Ok(HealthStatus::Progressing);
    };

    if let Some(conditions) = status.get("conditions").and_then(|c| c.as_array()) {
        for cond in conditions {
            let ctype = cond.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let cstatus = cond.get("status").and_then(|v| v.as_str()).unwrap_or("");
            if ctype == "Available" && cstatus == "True" {
                return Ok(HealthStatus::Healthy);
            }
        }
    }

    let replicas = status.get("replicas").and_then(|v| v.as_u64()).unwrap_or(0);
    let ready = status
        .get("readyReplicas")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if ready >= replicas && replicas > 0 {
        Ok(HealthStatus::Healthy)
    } else {
        Ok(HealthStatus::Progressing)
    }
}

fn check_statefulset_health(status: Option<&serde_json::Value>) -> Result<HealthStatus, KubeError> {
    let Some(status) = status else {
        return Ok(HealthStatus::Progressing);
    };
    let replicas = status.get("replicas").and_then(|v| v.as_u64()).unwrap_or(0);
    let ready = status
        .get("readyReplicas")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if ready >= replicas && replicas > 0 {
        Ok(HealthStatus::Healthy)
    } else {
        Ok(HealthStatus::Progressing)
    }
}

fn check_daemonset_health(status: Option<&serde_json::Value>) -> Result<HealthStatus, KubeError> {
    let Some(status) = status else {
        return Ok(HealthStatus::Progressing);
    };
    let desired = status
        .get("desiredNumberScheduled")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let ready = status
        .get("numberReady")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if ready >= desired && desired > 0 {
        Ok(HealthStatus::Healthy)
    } else {
        Ok(HealthStatus::Progressing)
    }
}

fn check_job_health(status: Option<&serde_json::Value>) -> Result<HealthStatus, KubeError> {
    let Some(status) = status else {
        return Ok(HealthStatus::Progressing);
    };
    if let Some(conditions) = status.get("conditions").and_then(|c| c.as_array()) {
        for cond in conditions {
            let ctype = cond.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let cstatus = cond.get("status").and_then(|v| v.as_str()).unwrap_or("");
            if ctype == "Complete" && cstatus == "True" {
                return Ok(HealthStatus::Healthy);
            }
            if ctype == "Failed" && cstatus == "True" {
                return Ok(HealthStatus::Degraded {
                    message: "Job failed".to_string(),
                });
            }
        }
    }
    Ok(HealthStatus::Progressing)
}

fn check_pvc_health(status: Option<&serde_json::Value>) -> Result<HealthStatus, KubeError> {
    let Some(status) = status else {
        return Ok(HealthStatus::Progressing);
    };
    let phase = status.get("phase").and_then(|v| v.as_str()).unwrap_or("");
    if phase == "Bound" {
        Ok(HealthStatus::Healthy)
    } else {
        Ok(HealthStatus::Progressing)
    }
}

fn check_crd_health(status: Option<&serde_json::Value>) -> Result<HealthStatus, KubeError> {
    check_generic_conditions(status)
}

fn check_generic_conditions(status: Option<&serde_json::Value>) -> Result<HealthStatus, KubeError> {
    let Some(status) = status else {
        return Ok(HealthStatus::Healthy);
    };
    if let Some(conditions) = status.get("conditions").and_then(|c| c.as_array()) {
        for cond in conditions {
            let ctype = cond.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let cstatus = cond.get("status").and_then(|v| v.as_str()).unwrap_or("");
            if (ctype == "Ready" || ctype == "Established") && cstatus == "True" {
                return Ok(HealthStatus::Healthy);
            }
        }
    }
    Ok(HealthStatus::Healthy)
}
