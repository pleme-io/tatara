//! Kube-client patch helpers.
//!
//! Status patches use `patch_status` (operates on the `/status` subresource).
//! Spec patches use `patch` (may trigger further reconciliation).
//!
//! All patches are `Patch::Merge` — simplest semantics; concurrent writes
//! are resolved by resourceVersion conflict retries at the controller level.

use chrono::Utc;
use kube::api::{Api, Patch, PatchParams, PostParams};
use kube::Error as KubeError;
use serde_json::{json, Value};

use tatara_process::phase::ProcessPhase;
use tatara_process::prelude::{Identity, Process, ProcessTable};
use tatara_process::table::ProcessTableSpec;

/// Merge-patch the status subresource of a Process.
pub async fn patch_process_status(
    api: &Api<Process>,
    name: &str,
    status_patch: Value,
) -> Result<Process, KubeError> {
    let body = json!({ "status": status_patch });
    api.patch_status(name, &PatchParams::default(), &Patch::Merge(&body))
        .await
}

/// Merge-patch the spec of a ProcessTable (we keep `nextSequence` in spec
/// for parity with convergence-controller; future refactor may move it to
/// status).
pub async fn patch_process_table_spec(
    api: &Api<ProcessTable>,
    name: &str,
    spec_patch: Value,
) -> Result<ProcessTable, KubeError> {
    let body = json!({ "spec": spec_patch });
    api.patch(name, &PatchParams::default(), &Patch::Merge(&body))
        .await
}

/// Ensure the cluster-scoped ProcessTable singleton exists, creating it
/// with defaults if absent.
pub async fn ensure_process_table(
    api: &Api<ProcessTable>,
    name: &str,
) -> Result<ProcessTable, KubeError> {
    if let Some(pt) = api.get_opt(name).await? {
        return Ok(pt);
    }
    let pt = ProcessTable {
        metadata: kube::api::ObjectMeta {
            name: Some(name.to_string()),
            ..Default::default()
        },
        spec: ProcessTableSpec {
            next_sequence: 1,
            parent_pid: None,
            dns_domain: None,
            dns_zone_id: None,
            max_depth: 0,
            max_children: 0,
            sigterm_timeout_seconds: 480,
            zombie_timeout_seconds: 600,
            orphan_reaping_enabled: true,
        },
        status: None,
    };
    api.create(&PostParams::default(), &pt).await
}

/// Common status patch builder — phase + phaseSince, optionally identity.
pub fn phase_status(phase: ProcessPhase, identity: Option<&Identity>) -> Value {
    let mut v = json!({
        "phase": phase,
        "phaseSince": Utc::now(),
    });
    if let Some(id) = identity {
        v["identity"] = serde_json::to_value(id).unwrap_or(Value::Null);
    }
    v
}

// ─── finalizer helpers ────────────────────────────────────────────────

/// Pure — compute the finalizer list after adding `target`.
/// Returns `None` if `target` is already present (idempotent).
pub fn add_finalizer(existing: &[String], target: &str) -> Option<Vec<String>> {
    if existing.iter().any(|f| f == target) {
        return None;
    }
    let mut new = existing.to_vec();
    new.push(target.to_string());
    Some(new)
}

/// Pure — compute the finalizer list after removing `target`.
/// Returns `None` if `target` is not present (idempotent).
pub fn remove_finalizer_from(existing: &[String], target: &str) -> Option<Vec<String>> {
    if !existing.iter().any(|f| f == target) {
        return None;
    }
    Some(existing.iter().filter(|f| *f != target).cloned().collect())
}

/// Add the tatara finalizer to a Process if not already present.
pub async fn ensure_finalizer(
    api: &Api<Process>,
    name: &str,
    p: &Process,
    target: &str,
) -> Result<bool, KubeError> {
    let existing = p.metadata.finalizers.clone().unwrap_or_default();
    let Some(new) = add_finalizer(&existing, target) else {
        return Ok(false);
    };
    let patch = json!({ "metadata": { "finalizers": new } });
    api.patch(name, &PatchParams::default(), &Patch::Merge(&patch))
        .await?;
    Ok(true)
}

/// Remove the tatara finalizer from a Process if present — allows K8s GC to proceed.
pub async fn remove_finalizer(
    api: &Api<Process>,
    name: &str,
    p: &Process,
    target: &str,
) -> Result<bool, KubeError> {
    let existing = p.metadata.finalizers.clone().unwrap_or_default();
    let Some(new) = remove_finalizer_from(&existing, target) else {
        return Ok(false);
    };
    let patch = json!({ "metadata": { "finalizers": new } });
    api.patch(name, &PatchParams::default(), &Patch::Merge(&patch))
        .await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_finalizer_appends_when_absent() {
        let existing = vec!["other.io/finalizer".to_string()];
        let result = add_finalizer(&existing, "tatara.pleme.io/process-finalizer").unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"tatara.pleme.io/process-finalizer".to_string()));
    }

    #[test]
    fn add_finalizer_idempotent_when_present() {
        let existing = vec!["tatara.pleme.io/process-finalizer".to_string()];
        assert!(add_finalizer(&existing, "tatara.pleme.io/process-finalizer").is_none());
    }

    #[test]
    fn remove_finalizer_strips_when_present() {
        let existing = vec![
            "a".to_string(),
            "tatara.pleme.io/process-finalizer".to_string(),
            "b".to_string(),
        ];
        let result = remove_finalizer_from(&existing, "tatara.pleme.io/process-finalizer").unwrap();
        assert_eq!(result, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn remove_finalizer_idempotent_when_absent() {
        let existing = vec!["other.io/x".to_string()];
        assert!(remove_finalizer_from(&existing, "tatara.pleme.io/process-finalizer").is_none());
    }
}
