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

/// Status patch builder — phase + phaseSince + operator-visible `message`.
///
/// The `phase + phaseSince + message` shape recurred at nine hand-authored
/// callsites (four `handle_*` transitions in `phase_machine.rs`, both
/// message-carrying `SignalEffect` arms in `signals.rs`, and the top-level
/// deletion-preempt in `controller.rs`) before this primitive existed;
/// each site inlined `json!({ "phase": ..., "phaseSince": Utc::now(),
/// "message": ... })` verbatim. Post-lift the three-slot shape lives at
/// ONE substrate primitive — a future field addition (`by:` for the
/// signal source, `transitionCount:` for a diagnostic counter,
/// structured message envelope) lands here and every downstream
/// transition inherits it mechanically. `impl Into<String>` accepts
/// both `&'static str` literal reasons and `format!(...)`-owned strings
/// without widening the signature.
pub fn phase_status_msg(phase: ProcessPhase, message: impl Into<String>) -> Value {
    json!({
        "phase": phase,
        "phaseSince": Utc::now(),
        "message": message.into(),
    })
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

    // ─── phase_status / phase_status_msg substrate pins ────────────────────
    //
    // The three-slot `phase + phaseSince + message` shape recurred at nine
    // hand-authored callsites before `phase_status_msg` closed it. These
    // pins bind the primitive at fail-before-pass-after granularity so a
    // regression that dropped a slot, renamed a key, or reshaped a value
    // surfaces here rather than as silent operator-facing drift at every
    // downstream transition.

    #[test]
    fn phase_status_msg_composes_the_three_slots() {
        let v = phase_status_msg(ProcessPhase::Execing, "dependencies satisfied");
        let obj = v.as_object().expect("object");
        // All three slots present, no extras.
        assert_eq!(obj.len(), 3);
        assert_eq!(
            obj.get("phase").and_then(Value::as_str),
            Some("Execing"),
            "phase key present and serialises as the discriminant string"
        );
        assert_eq!(
            obj.get("message").and_then(Value::as_str),
            Some("dependencies satisfied"),
            "message key present and rides through verbatim"
        );
        assert!(
            obj.get("phaseSince")
                .and_then(Value::as_str)
                .is_some_and(|s| chrono::DateTime::parse_from_rfc3339(s).is_ok()),
            "phaseSince key present and is a valid RFC-3339 timestamp"
        );
    }

    #[test]
    fn phase_status_msg_accepts_static_str_and_owned_string() {
        // &'static str literal — the shape at every non-format! site
        // (e.g. controller.rs "deletion requested",
        // phase_machine.rs "dependencies satisfied").
        let a = phase_status_msg(ProcessPhase::Exiting, "deletion requested");
        assert_eq!(
            a.get("message").and_then(Value::as_str),
            Some("deletion requested")
        );
        // Owned String — the shape at every format!(...) site
        // (e.g. phase_machine.rs `format!("releasing → {next} — {reason}")`).
        let owned: String = format!("releasing → {} — {}", ProcessPhase::Zombie, "TTL expired");
        let b = phase_status_msg(ProcessPhase::Releasing, owned);
        assert_eq!(
            b.get("message").and_then(Value::as_str),
            Some("releasing → Zombie — TTL expired")
        );
    }

    #[test]
    fn phase_status_msg_stamps_phase_since_at_call_time() {
        let before = Utc::now();
        let v = phase_status_msg(ProcessPhase::Reconverging, "drift");
        let after = Utc::now();
        let stamped = v
            .get("phaseSince")
            .and_then(Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .expect("phaseSince is an RFC-3339 timestamp");
        let stamped_utc = stamped.with_timezone(&chrono::Utc);
        assert!(
            (before..=after).contains(&stamped_utc),
            "phaseSince ({stamped_utc}) must land in [{before}, {after}] — the primitive stamps at call time"
        );
    }

    #[test]
    fn phase_status_bare_differs_from_phase_status_msg_only_by_message_key() {
        // The five bare `handle_*` transitions (reconverging→execing,
        // exiting→zombie, failed→zombie, zombie→reaped, signal
        // transition) collapse onto `phase_status(phase, None)` — same
        // primitive, no `message` key. This pin catches a regression
        // where the bare variant accidentally acquired a `message` slot
        // (or the msg variant accidentally dropped it).
        let bare = phase_status(ProcessPhase::Zombie, None);
        let msg = phase_status_msg(ProcessPhase::Zombie, "irrelevant");
        assert_eq!(bare.as_object().unwrap().len(), 2);
        assert_eq!(msg.as_object().unwrap().len(), 3);
        assert!(bare.get("message").is_none());
        assert!(msg.get("message").is_some());
        // Both agree on the phase discriminant they were built with.
        assert_eq!(bare.get("phase"), msg.get("phase"));
    }
}
