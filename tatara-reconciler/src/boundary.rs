//! Boundary condition evaluator — the VERIFY half of the convergence loop.
//!
//! Evaluates the `ConditionKind` variants against live cluster state:
//! - `ProcessPhase`:           lookup the referenced Process, compare phase
//! - `KustomizationHealthy`:   fetch the Kustomization, read `status.conditions[Ready]`
//! - `HelmReleaseReleased`:    same, for `HelmRelease`
//! - `PromQL`:                 stub (returns Unknown) — needs a metrics client
//! - `Cel`:                    stub (returns Unknown) — needs a CEL runtime
//! - `NixEval`:                stub (returns Unknown) — needs tatara-engine
//! - `JobAttested`:            Job.status.succeeded >= 1; optional receipt
//!                             ConfigMap verification
//! - `ClosedLoopAuth`:         JobAttested + BLAKE3 receipt shape verified
//!                             (the canonical postcondition for any system
//!                             that can produce credentials for its own
//!                             client under test — e.g. Akeyless SaaS
//!                             issuing secrets to its bundled Gateway)
//!
//! `check_depends_on` reuses the `ProcessPhase` evaluator and returns unmet
//! dependencies structured for UX messaging.

use anyhow::{anyhow, Result};
use kube::{Api, Client};
use serde::Deserialize;
use serde_json::Value;

use tatara_process::boundary::{Condition, ConditionKind};
use tatara_process::phase::ProcessPhase;
use tatara_process::prelude::Process;
use tatara_process::receipt::{ReceiptEnvelope, ReceiptError};

use crate::ssapply;

/// Result of a single boundary predicate evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Satisfaction {
    /// The predicate holds.
    Satisfied,
    /// The predicate does not hold; `String` is a user-facing reason.
    Unsatisfied(String),
    /// Evaluator could not determine (missing data, unimplemented kind, etc.).
    Unknown(String),
}

impl Satisfaction {
    pub fn is_satisfied(&self) -> bool {
        matches!(self, Self::Satisfied)
    }
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Satisfied => None,
            Self::Unsatisfied(m) | Self::Unknown(m) => Some(m),
        }
    }
}

// ── typed params per kind ────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProcessPhaseParams {
    process_ref: String,
    #[serde(default)]
    namespace: Option<String>,
    phase: ProcessPhase,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NamedResourceParams {
    name: String,
    #[serde(default)]
    namespace: Option<String>,
}

// ── entry point ──────────────────────────────────────────────────────

/// Evaluate a single boundary condition against the cluster.
pub async fn evaluate(
    client: Client,
    process: &Process,
    condition: &Condition,
) -> Result<Satisfaction> {
    let default_ns = process.metadata.namespace.as_deref().unwrap_or("default");
    match condition.kind {
        ConditionKind::ProcessPhase => {
            evaluate_process_phase(client, default_ns, &condition.params).await
        }
        ConditionKind::KustomizationHealthy => {
            evaluate_flux_ready(
                client,
                default_ns,
                &condition.params,
                "kustomize.toolkit.fluxcd.io/v1",
                "Kustomization",
            )
            .await
        }
        ConditionKind::HelmReleaseReleased => {
            evaluate_flux_ready(
                client,
                default_ns,
                &condition.params,
                "helm.toolkit.fluxcd.io/v2",
                "HelmRelease",
            )
            .await
        }
        ConditionKind::PromQL => Ok(Satisfaction::Unknown(
            "PromQL evaluator not yet implemented".into(),
        )),
        ConditionKind::Cel => Ok(Satisfaction::Unknown(
            "CEL evaluator not yet implemented".into(),
        )),
        ConditionKind::NixEval => Ok(Satisfaction::Unknown(
            "NixEval evaluator not yet implemented".into(),
        )),
        ConditionKind::JobAttested => {
            evaluate_job_attested(client, default_ns, &condition.params, process).await
        }
        ConditionKind::ClosedLoopAuth => {
            evaluate_closed_loop_auth(client, default_ns, &condition.params, process).await
        }
    }
}

// ── JobAttested + ClosedLoopAuth typed evaluators ────────────────────

/// `JobAttested` params:
/// ```json
/// { "name": "<job-name>", "namespace": "<ns>",
///   "expectReceipt": true,                        // default false
///   "receiptConfigMap": "<cm-name>" }             // defaults to <name>-receipt
/// ```
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobAttestedParams {
    name: String,
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    expect_receipt: bool,
    #[serde(default)]
    receipt_config_map: Option<String>,
}

/// Read a Kubernetes `batch/v1` Job's status and decide.
async fn evaluate_job_attested(
    client: Client,
    default_ns: &str,
    params: &Value,
    _process: &Process,
) -> Result<Satisfaction> {
    let parsed: JobAttestedParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return Ok(Satisfaction::Unknown(format!(
                "JobAttested params invalid: {e}"
            )))
        }
    };
    let ns = parsed.namespace.as_deref().unwrap_or(default_ns);
    let job_status = fetch_job_status(client.clone(), ns, &parsed.name).await?;
    let job_status = match job_status {
        JobLookup::Found(s) => s,
        JobLookup::Missing => {
            return Ok(Satisfaction::Unsatisfied(format!(
                "Job {ns}/{} not found",
                parsed.name
            )))
        }
    };

    if job_status.failed > 0 {
        return Ok(Satisfaction::Unsatisfied(format!(
            "Job {ns}/{} failed (status.failed={})",
            parsed.name, job_status.failed
        )));
    }
    if job_status.succeeded < 1 {
        return Ok(Satisfaction::Unsatisfied(format!(
            "Job {ns}/{} still running (succeeded={}, active={})",
            parsed.name, job_status.succeeded, job_status.active
        )));
    }

    if !parsed.expect_receipt {
        return Ok(Satisfaction::Satisfied);
    }
    let cm_name = parsed
        .receipt_config_map
        .clone()
        .unwrap_or_else(|| format!("{}-receipt", parsed.name));
    match verify_receipt_cm(client, ns, &cm_name, None).await? {
        ReceiptVerdict::Ok(_) => Ok(Satisfaction::Satisfied),
        ReceiptVerdict::Missing => Ok(Satisfaction::Unsatisfied(format!(
            "Job {ns}/{} succeeded but receipt ConfigMap {ns}/{cm_name} missing",
            parsed.name
        ))),
        ReceiptVerdict::Malformed(why) => Ok(Satisfaction::Unsatisfied(format!(
            "Job {ns}/{} receipt malformed: {why}",
            parsed.name
        ))),
    }
}

/// `ClosedLoopAuth` params — the typed shape is in `tatara-process`'s
/// boundary.rs doc; the reconciler reads the optional Job + ConfigMap
/// names and falls back to deterministic defaults derived from the
/// owning Process's name.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClosedLoopAuthParams {
    #[serde(default)]
    namespace: Option<String>,
    /// Optional override; defaults to `<process>-closed-loop-probe`.
    #[serde(default)]
    job_name: Option<String>,
    /// Optional override; defaults to `<job-name>-receipt`.
    #[serde(default)]
    receipt_config_map: Option<String>,
    /// Expected three-pillar BLAKE3 root. When omitted, we only verify
    /// shape; the reconciler chains the observed root into the
    /// Process's attestation regardless.
    #[serde(default)]
    expected_root: Option<String>,
    /// Free-form remaining keys (issuer/consumer/jwkSource/probeImage/etc.)
    /// — the chart deploying the probe consumes these; the reconciler
    /// itself does not need them to verify the receipt.
    #[serde(default, flatten)]
    _extra: std::collections::BTreeMap<String, Value>,
}

async fn evaluate_closed_loop_auth(
    client: Client,
    default_ns: &str,
    params: &Value,
    process: &Process,
) -> Result<Satisfaction> {
    let parsed: ClosedLoopAuthParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return Ok(Satisfaction::Unknown(format!(
                "ClosedLoopAuth params invalid: {e}"
            )))
        }
    };
    let ns = parsed.namespace.as_deref().unwrap_or(default_ns);
    let process_name = process
        .metadata
        .name
        .as_deref()
        .unwrap_or("unnamed-process");
    let job_name = parsed
        .job_name
        .clone()
        .unwrap_or_else(|| format!("{process_name}-closed-loop-probe"));
    let cm_name = parsed
        .receipt_config_map
        .clone()
        .unwrap_or_else(|| format!("{job_name}-receipt"));

    // 1. The probe Job must have succeeded.
    let job_status = fetch_job_status(client.clone(), ns, &job_name).await?;
    let job_status = match job_status {
        JobLookup::Found(s) => s,
        JobLookup::Missing => {
            return Ok(Satisfaction::Unsatisfied(format!(
                "closed-loop probe Job {ns}/{job_name} not found"
            )))
        }
    };
    if job_status.failed > 0 {
        return Ok(Satisfaction::Unsatisfied(format!(
            "closed-loop probe Job {ns}/{job_name} failed (status.failed={})",
            job_status.failed
        )));
    }
    if job_status.succeeded < 1 {
        return Ok(Satisfaction::Unsatisfied(format!(
            "closed-loop probe Job {ns}/{job_name} still running"
        )));
    }

    // 2. The receipt ConfigMap must exist and parse.
    match verify_receipt_cm(client, ns, &cm_name, parsed.expected_root.as_deref()).await? {
        ReceiptVerdict::Ok(_root) => Ok(Satisfaction::Satisfied),
        ReceiptVerdict::Missing => Ok(Satisfaction::Unsatisfied(format!(
            "closed-loop receipt ConfigMap {ns}/{cm_name} missing"
        ))),
        ReceiptVerdict::Malformed(why) => Ok(Satisfaction::Unsatisfied(format!(
            "closed-loop receipt malformed: {why}"
        ))),
    }
}

#[derive(Debug)]
enum JobLookup {
    Missing,
    Found(JobStatusView),
}

#[derive(Debug, Default)]
struct JobStatusView {
    succeeded: i64,
    failed: i64,
    active: i64,
}

async fn fetch_job_status(client: Client, ns: &str, name: &str) -> Result<JobLookup> {
    let obj = ssapply::fetch(client, ns, "batch/v1", "Job", name)
        .await
        .map_err(|e| anyhow!("fetch Job {ns}/{name}: {e}"))?;
    let Some(obj) = obj else {
        return Ok(JobLookup::Missing);
    };
    let status = obj.data.get("status").cloned().unwrap_or(Value::Null);
    let mut view = JobStatusView::default();
    if let Some(s) = status.get("succeeded").and_then(|v| v.as_i64()) {
        view.succeeded = s;
    }
    if let Some(f) = status.get("failed").and_then(|v| v.as_i64()) {
        view.failed = f;
    }
    if let Some(a) = status.get("active").and_then(|v| v.as_i64()) {
        view.active = a;
    }
    Ok(JobLookup::Found(view))
}

#[derive(Debug, PartialEq, Eq)]
enum ReceiptVerdict {
    Missing,
    Malformed(String),
    /// Composed root — for the attestation chain.
    Ok(String),
}

/// Fetch the receipt ConfigMap, look up `data['receipt.json']` (or
/// `data['receipt.yaml']` as a fallback), and delegate parsing to the
/// typed `ReceiptEnvelope::parse_either` in `tatara-process`.
async fn verify_receipt_cm(
    client: Client,
    ns: &str,
    name: &str,
    expected_root: Option<&str>,
) -> Result<ReceiptVerdict> {
    let obj = ssapply::fetch(client, ns, "v1", "ConfigMap", name)
        .await
        .map_err(|e| anyhow!("fetch ConfigMap {ns}/{name}: {e}"))?;
    let Some(obj) = obj else {
        return Ok(ReceiptVerdict::Missing);
    };
    let data = obj.data.get("data");
    let payload = data
        .and_then(|d| d.get("receipt.json"))
        .or_else(|| data.and_then(|d| d.get("receipt.yaml")))
        .and_then(|v| v.as_str());
    let Some(payload) = payload else {
        return Ok(ReceiptVerdict::Malformed(
            "ConfigMap missing data['receipt.json' | 'receipt.yaml'] string key".into(),
        ));
    };
    Ok(parse_receipt_payload(payload, expected_root))
}

/// Pure parser — delegates to the typed `ReceiptEnvelope::parse_either`,
/// then runs the `expected_root` check separately. Maps typed
/// `ReceiptError` variants into `ReceiptVerdict::Malformed` with a
/// stable, operator-friendly string so existing UX is preserved.
fn parse_receipt_payload(payload: &str, expected_root: Option<&str>) -> ReceiptVerdict {
    let envelope = match ReceiptEnvelope::parse_either(payload) {
        Ok(e) => e,
        Err(err) => return ReceiptVerdict::Malformed(receipt_error_message(&err)),
    };
    match envelope.expect_root(expected_root) {
        Ok(root) => ReceiptVerdict::Ok(root.to_string()),
        Err(err) => ReceiptVerdict::Malformed(receipt_error_message(&err)),
    }
}

/// Lower a `ReceiptError` to the same operator-visible strings the
/// older hand-rolled parser surfaced, so dashboards / alerts that grep
/// for these messages keep working.
fn receipt_error_message(err: &ReceiptError) -> String {
    match err {
        ReceiptError::InvalidJson(m) => format!("invalid JSON: {m}"),
        ReceiptError::InvalidYaml(m) => format!("invalid YAML: {m}"),
        ReceiptError::WrongVersion(v) => format!("version != tatara-receipt/v1 (got {v:?})"),
        ReceiptError::MissingField(f) => format!("missing '{f}' string field"),
        ReceiptError::EmptyKind => "kind is empty".into(),
        ReceiptError::RootMismatch { got, want } => {
            format!("composed_root mismatch (got {got}, want {want})")
        }
    }
}

// ── per-kind evaluators ──────────────────────────────────────────────

async fn evaluate_process_phase(
    client: Client,
    default_ns: &str,
    params: &Value,
) -> Result<Satisfaction> {
    let parsed: ProcessPhaseParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return Ok(Satisfaction::Unknown(format!(
                "ProcessPhase params invalid: {e}"
            )))
        }
    };
    let ns = parsed.namespace.as_deref().unwrap_or(default_ns);
    let api: Api<Process> = Api::namespaced(client, ns);
    let target = match api
        .get_opt(&parsed.process_ref)
        .await
        .map_err(|e| anyhow!("fetch process {ns}/{}: {e}", parsed.process_ref))?
    {
        Some(t) => t,
        None => {
            return Ok(Satisfaction::Unsatisfied(format!(
                "process {}/{} not found",
                ns, parsed.process_ref
            )))
        }
    };
    let actual = target
        .status
        .as_ref()
        .map(|s| s.phase)
        .unwrap_or(ProcessPhase::Pending);
    if phase_reached(actual, parsed.phase) {
        Ok(Satisfaction::Satisfied)
    } else {
        Ok(Satisfaction::Unsatisfied(format!(
            "{}/{} is {actual}; need at least {}",
            ns, parsed.process_ref, parsed.phase
        )))
    }
}

async fn evaluate_flux_ready(
    client: Client,
    default_ns: &str,
    params: &Value,
    api_version: &str,
    kind: &str,
) -> Result<Satisfaction> {
    let parsed: NamedResourceParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return Ok(Satisfaction::Unknown(format!("{kind} params invalid: {e}"))),
    };
    let ns = parsed.namespace.as_deref().unwrap_or(default_ns);
    let obj = ssapply::fetch(client, ns, api_version, kind, &parsed.name)
        .await
        .map_err(|e| anyhow!("fetch {kind} {ns}/{}: {e}", parsed.name))?;
    match obj {
        None => Ok(Satisfaction::Unsatisfied(format!(
            "{kind} {}/{} not found",
            ns, parsed.name
        ))),
        Some(dyn_obj) => match ssapply::ready_condition(&dyn_obj) {
            ssapply::ReadyState::Ready => Ok(Satisfaction::Satisfied),
            ssapply::ReadyState::NotReady(m) => Ok(Satisfaction::Unsatisfied(
                m.unwrap_or_else(|| format!("{kind} not ready")),
            )),
            ssapply::ReadyState::Unknown => {
                Ok(Satisfaction::Unknown(format!("{kind} condition unknown")))
            }
        },
    }
}

// ── phase ordering (pure) ────────────────────────────────────────────

/// Rank for the "must reach" comparison. Dead phases rank 0 to prevent
/// terminating processes from satisfying any live-phase requirement.
pub fn phase_rank(p: ProcessPhase) -> u8 {
    match p {
        ProcessPhase::Pending => 0,
        ProcessPhase::Forking => 1,
        ProcessPhase::Execing => 2,
        ProcessPhase::Running | ProcessPhase::Reconverging => 3,
        ProcessPhase::Attested => 4,
        ProcessPhase::Releasing
        | ProcessPhase::Exiting
        | ProcessPhase::Failed
        | ProcessPhase::Zombie
        | ProcessPhase::Reaped => 0,
    }
}

/// Has `actual` reached the required minimum phase?
pub fn phase_reached(actual: ProcessPhase, required: ProcessPhase) -> bool {
    phase_rank(actual) >= phase_rank(required)
}

// ── depends_on ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct UnmetDependency {
    pub name: String,
    pub namespace: String,
    pub required: ProcessPhase,
    pub actual: Option<ProcessPhase>,
    pub message: String,
}

/// Check every `spec.dependsOn` entry against live cluster state.
/// Returns the list of unmet dependencies (empty = proceed).
pub async fn check_depends_on(client: Client, process: &Process) -> Result<Vec<UnmetDependency>> {
    let default_ns = process.metadata.namespace.as_deref().unwrap_or("default");
    let mut unmet = Vec::new();

    for dep in &process.spec.depends_on {
        let ns = dep.namespace.as_deref().unwrap_or(default_ns);
        let required: ProcessPhase = dep.must_reach.into();
        let api: Api<Process> = Api::namespaced(client.clone(), ns);
        match api.get_opt(&dep.name).await {
            Ok(Some(target)) => {
                let actual = target.status.as_ref().map(|s| s.phase);
                let actual_phase = actual.unwrap_or(ProcessPhase::Pending);
                if !phase_reached(actual_phase, required) {
                    unmet.push(UnmetDependency {
                        name: dep.name.clone(),
                        namespace: ns.to_string(),
                        required,
                        actual,
                        message: format!("{}/{} is {actual_phase}; need {required}", ns, dep.name),
                    });
                }
            }
            Ok(None) => unmet.push(UnmetDependency {
                name: dep.name.clone(),
                namespace: ns.to_string(),
                required,
                actual: None,
                message: format!("{}/{} not found", ns, dep.name),
            }),
            Err(e) => unmet.push(UnmetDependency {
                name: dep.name.clone(),
                namespace: ns.to_string(),
                required,
                actual: None,
                message: format!("error fetching {}/{}: {e}", ns, dep.name),
            }),
        }
    }
    Ok(unmet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_rank_ordering_ascends_through_lifecycle() {
        assert!(phase_rank(ProcessPhase::Pending) < phase_rank(ProcessPhase::Forking));
        assert!(phase_rank(ProcessPhase::Forking) < phase_rank(ProcessPhase::Execing));
        assert!(phase_rank(ProcessPhase::Execing) < phase_rank(ProcessPhase::Running));
        assert!(phase_rank(ProcessPhase::Running) < phase_rank(ProcessPhase::Attested));
    }

    #[test]
    fn reconverging_has_same_rank_as_running() {
        assert_eq!(
            phase_rank(ProcessPhase::Reconverging),
            phase_rank(ProcessPhase::Running)
        );
    }

    #[test]
    fn attested_satisfies_running_requirement() {
        assert!(phase_reached(ProcessPhase::Attested, ProcessPhase::Running));
        assert!(phase_reached(ProcessPhase::Running, ProcessPhase::Running));
    }

    #[test]
    fn running_does_not_satisfy_attested() {
        assert!(!phase_reached(
            ProcessPhase::Running,
            ProcessPhase::Attested
        ));
        assert!(!phase_reached(ProcessPhase::Pending, ProcessPhase::Running));
    }

    // ── receipt parser (pure) ────────────────────────────────────────

    fn valid_receipt() -> String {
        // Compose root deterministically using the same domain tag as
        // ReceiptEnvelope so the generated payload parses cleanly.
        let env = ReceiptEnvelope::build("closed-loop-auth", "aaaa", "bbbb", "cccc", None);
        serde_json::to_string(&env).unwrap()
    }

    #[test]
    fn valid_receipt_parses() {
        let payload = valid_receipt();
        let v = parse_receipt_payload(&payload, None);
        assert!(matches!(v, ReceiptVerdict::Ok(_)));
    }

    #[test]
    fn expected_root_match_succeeds_mismatch_fails() {
        let env = ReceiptEnvelope::build("closed-loop-auth", "aaaa", "bbbb", "cccc", None);
        let payload = serde_json::to_string(&env).unwrap();
        let root = env.composed_root.clone();

        let v = parse_receipt_payload(&payload, Some(&root));
        assert!(matches!(v, ReceiptVerdict::Ok(_)));

        let v = parse_receipt_payload(&payload, Some("nope"));
        assert!(matches!(v, ReceiptVerdict::Malformed(ref m) if m.contains("composed_root mismatch")));
    }

    #[test]
    fn missing_version_is_malformed() {
        // serde with deny_unknown_fields rejects this before our shape check —
        // surfaces as "invalid JSON: missing field `version`".
        let s = r#"{"composed_root":"x","intent_hash":"a","artifact_hash":"b","control_hash":"c","kind":"x","generated_at":"2026-05-19T12:00:00Z"}"#;
        let v = parse_receipt_payload(s, None);
        assert!(
            matches!(v, ReceiptVerdict::Malformed(ref m) if m.contains("version")),
            "expected version-related malformed message, got {v:?}"
        );
    }

    #[test]
    fn wrong_version_is_malformed() {
        let mut payload: Value = serde_json::from_str(&valid_receipt()).unwrap();
        payload["version"] = Value::String("tatara-receipt/v2".into());
        let v = parse_receipt_payload(&payload.to_string(), None);
        assert!(matches!(v, ReceiptVerdict::Malformed(ref m) if m.contains("version != tatara-receipt/v1")));
    }

    #[test]
    fn missing_any_pillar_is_malformed() {
        for pillar in ["intent_hash", "artifact_hash", "control_hash"] {
            let mut payload: Value = serde_json::from_str(&valid_receipt()).unwrap();
            payload.as_object_mut().unwrap().remove(pillar);
            let v = parse_receipt_payload(&payload.to_string(), None);
            assert!(
                matches!(v, ReceiptVerdict::Malformed(ref m) if m.contains(pillar)),
                "expected malformed for missing '{pillar}', got {v:?}"
            );
        }
    }

    #[test]
    fn missing_composed_root_is_malformed() {
        let mut payload: Value = serde_json::from_str(&valid_receipt()).unwrap();
        payload.as_object_mut().unwrap().remove("composed_root");
        let v = parse_receipt_payload(&payload.to_string(), None);
        assert!(matches!(v, ReceiptVerdict::Malformed(ref m) if m.contains("composed_root")));
    }

    #[test]
    fn invalid_json_is_malformed() {
        let v = parse_receipt_payload("not json", None);
        assert!(matches!(v, ReceiptVerdict::Malformed(ref m) if m.to_lowercase().contains("invalid")));
    }

    #[test]
    fn missing_kind_is_malformed() {
        let mut payload: Value = serde_json::from_str(&valid_receipt()).unwrap();
        payload.as_object_mut().unwrap().remove("kind");
        let v = parse_receipt_payload(&payload.to_string(), None);
        assert!(matches!(v, ReceiptVerdict::Malformed(ref m) if m.contains("kind")));
    }

    #[test]
    fn yaml_payload_parses_via_either() {
        let env = ReceiptEnvelope::build("db-migration", "aaaa", "bbbb", "cccc", None);
        let yaml = serde_yaml::to_string(&env).unwrap();
        let v = parse_receipt_payload(&yaml, None);
        assert!(matches!(v, ReceiptVerdict::Ok(_)));
    }

    #[test]
    fn terminal_phases_satisfy_nothing() {
        for dead in [
            ProcessPhase::Exiting,
            ProcessPhase::Failed,
            ProcessPhase::Zombie,
            ProcessPhase::Reaped,
        ] {
            assert!(!phase_reached(dead, ProcessPhase::Running));
            assert!(!phase_reached(dead, ProcessPhase::Attested));
        }
    }

    #[test]
    fn satisfaction_reports_correctly() {
        assert!(Satisfaction::Satisfied.is_satisfied());
        assert!(!Satisfaction::Unsatisfied("x".into()).is_satisfied());
        assert!(!Satisfaction::Unknown("y".into()).is_satisfied());
        assert_eq!(
            Satisfaction::Unsatisfied("why".into()).message(),
            Some("why")
        );
        assert_eq!(Satisfaction::Satisfied.message(), None);
    }
}
