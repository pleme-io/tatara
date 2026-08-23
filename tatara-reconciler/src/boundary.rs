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
//!                             client under test — e.g. a secrets
//!                             backend issuing creds to its bundled client)
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
#[cfg(test)]
use tatara_process::receipt::ReceiptKind;
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

/// Deserialize the axis-typed params carrier `T` from a boundary
/// condition's `params: serde_json::Value`, projecting a serde-level
/// rejection into the operator-facing
/// [`Satisfaction::Unknown("<kind_label> params invalid: {e}")`]
/// diagnostic every kind-typed evaluator on this file pre-lift restated
/// verbatim at its own site.
///
/// Pre-lift the FOUR kind-typed evaluators
/// ([`evaluate_process_phase`] on the `ProcessPhase` axis,
/// [`evaluate_flux_ready`] on the `Kustomization` / `HelmRelease` axes,
/// [`evaluate_job_attested`] on the `JobAttested` axis,
/// [`evaluate_closed_loop_auth`] on the `ClosedLoopAuth` axis) each
/// hand-authored the same three-invariant scaffold at their own local
/// site: (1) clone the `Value`, (2) call `serde_json::from_value::<T>`
/// with the carrier's turbofish, (3) on Err wrap the syn-error into
/// `Satisfaction::Unknown` with a `"<kind_label> params invalid: {e}"`
/// prefix. FOUR byte-for-byte identical `let parsed: <T> = match
/// serde_json::from_value(params.clone()) { … }` blocks past the
/// PRIME-DIRECTIVE ≥ 2 duplication threshold, differing only in the
/// axis-typed carrier `T` and the `&str` kind label each threaded
/// through the SAME scaffold.
///
/// Post-lift the three shared invariants live at ONE substrate primitive
/// here; each evaluator threads its axis-typed carrier through the
/// turbofish and its kind label through the `&str` slot, and the Err
/// arm rides through `parse_condition_params(...).map_err(|s| ...)?`
/// at each callsite's own `Result<Satisfaction>` early-return path. A
/// regression that drifted the diagnostic wording (a swapped `params
/// invalid` phrase, a missing kind-label prefix, a promoted variant on
/// the `Satisfaction` axis) at ONE evaluator surfaces at the sibling
/// test [`parse_condition_params_tests`] rather than as silent
/// operator-facing drift across the four kind-typed evaluators.
///
/// The `kind_label` parameter is a `&str` (not `&'static str`) because
/// [`evaluate_flux_ready`] threads its `kind: &str` parameter (whose
/// call sites pass `"Kustomization"` / `"HelmRelease"` — both statically
/// known but locally borrowed through the outer fn signature) through
/// this slot. Every other callsite passes a bare `&'static str` literal
/// (`"ProcessPhase"`, `"JobAttested"`, `"ClosedLoopAuth"`); the `&str`
/// bound admits both. A future ConditionKind whose evaluator wants to
/// tag its diagnostic with a dynamic axis (a hypothetical
/// `PromQL(query_label)` variant that names the failing query in the
/// diagnostic) lands as ONE new callsite through the SAME primitive
/// with a locally-borrowed `&str`, no widening of the primitive's
/// signature.
///
/// The `T: serde::de::DeserializeOwned` bound rides the caller's
/// turbofish, matching how the substrate's own trait-dispatch primitive
/// [`tatara_lisp::domain::DeserializeKwarg`] binds its axis-agnostic
/// `T: DeserializeOwned` slot. All four workspace-shipped params
/// carriers (`ProcessPhaseParams` / `NamedResourceParams` /
/// `JobAttestedParams` / `ClosedLoopAuthParams`) are `#[derive(Deserialize)]`
/// with owned `String` slots, so `DeserializeOwned` holds mechanically.
///
/// Future structural promotion of the emitted diagnostic (a caller-
/// supplied span, a `note = "help: …"` chain, an `expected: <shape>`
/// hint drawn from the carrier's `serde` schema, a structured
/// `Satisfaction::Unknown { kind, source }` promotion) lands at ONE
/// substrate primitive here — all four kind-typed evaluators and every
/// future new ConditionKind evaluator pick up the upgrade mechanically,
/// with no per-evaluator hand-edit.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// three-invariant scaffold recurred at four kind-typed evaluators
/// well past the PRIME-DIRECTIVE ≥ 2 duplication trigger, and is
/// lifted to ONE owner here). THEORY.md §II.1 invariant 5 (composition
/// preserves proofs — the four axis-typed evaluators now compose
/// structurally through ONE primitive; a regression that drifted the
/// wording or the Satisfaction variant at ONE axis surfaces at
/// [`parse_condition_params_tests`] rather than as silent drift at
/// every downstream evaluator with a params carrier on that axis).
fn parse_condition_params<T: serde::de::DeserializeOwned>(
    kind_label: &str,
    params: &Value,
) -> Result<T, Satisfaction> {
    serde_json::from_value(params.clone())
        .map_err(|e| Satisfaction::Unknown(format!("{kind_label} params invalid: {e}")))
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
        // Stub evaluators — `ConditionKind::stub_message` owns each
        // operator-facing string in `tatara-process::boundary`, so the
        // three "not yet implemented" arms here collapse to one site
        // that delegates to the typed projection. Adding (or
        // promoting) a stub kind lands at `stub_message`, not here.
        // The OR-pattern keeps exhaustiveness — the compiler still
        // forces every `ConditionKind` to reach a branch.
        ConditionKind::PromQL | ConditionKind::Cel | ConditionKind::NixEval => {
            Ok(Satisfaction::Unknown(
                condition
                    .kind
                    .stub_message()
                    .expect("is_stub() iff stub_message().is_some()")
                    .into(),
            ))
        }
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
    let parsed: JobAttestedParams = match parse_condition_params("JobAttested", params) {
        Ok(p) => p,
        Err(unk) => return Ok(unk),
    };
    let ns = parsed.namespace.as_deref().unwrap_or(default_ns);
    if let Err(unsat) = require_succeeded_job(client.clone(), ns, &parsed.name, "Job").await? {
        return Ok(unsat);
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
    let parsed: ClosedLoopAuthParams = match parse_condition_params("ClosedLoopAuth", params) {
        Ok(p) => p,
        Err(unk) => return Ok(unk),
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
    if let Err(unsat) =
        require_succeeded_job(client.clone(), ns, &job_name, "closed-loop probe Job").await?
    {
        return Ok(unsat);
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

#[derive(Debug, PartialEq, Eq)]
enum JobLookup {
    Missing,
    Found(JobStatusView),
}

#[derive(Debug, Default, PartialEq, Eq)]
struct JobStatusView {
    succeeded: i64,
    failed: i64,
    active: i64,
}

/// Pure projection of a fetched Job's status into an operator-facing
/// [`Satisfaction`] diagnostic keyed on the caller's `label`.
///
/// Pre-lift the TWO postcondition evaluators
/// ([`evaluate_job_attested`] on the `JobAttested` axis and
/// [`evaluate_closed_loop_auth`] on the `ClosedLoopAuth` axis) each
/// hand-authored the same three-invariant scaffold at their own local
/// site: (1) [`JobLookup::Missing`] projects to
/// `Satisfaction::Unsatisfied("<label> {ns}/{name} not found")`,
/// (2) `status.failed > 0` projects to
/// `Satisfaction::Unsatisfied("<label> {ns}/{name} failed
/// (status.failed={n})")`, (3) `status.succeeded < 1` projects to
/// `Satisfaction::Unsatisfied("<label> {ns}/{name} still running
/// (succeeded={s}, active={a})")`. TWO byte-for-byte identical
/// `let job_status = match fetch_job_status(...) { … }; if
/// job_status.failed > 0 { … } if job_status.succeeded < 1 { … }`
/// blocks past the PRIME-DIRECTIVE ≥ 2 duplication threshold,
/// differing only in the label prefix (`"Job"` vs `"closed-loop
/// probe Job"`) each threaded through the SAME scaffold.
///
/// Post-lift the three shared invariants live at ONE substrate
/// primitive here (the pure projection) plus one async peer
/// [`require_succeeded_job`] that owns the fetch. A regression
/// that drifted the diagnostic wording (a swapped counter phrase,
/// a missing label prefix, a promoted variant on the
/// [`Satisfaction`] axis) at ONE evaluator surfaces at
/// [`classify_job_status_tests`] rather than as silent
/// operator-facing drift across the two postcondition evaluators.
///
/// The lift also **strictly widens** the closed-loop path's "still
/// running" diagnostic: pre-lift it read
/// `"closed-loop probe Job {ns}/{name} still running"` (no
/// counters); post-lift it inherits the JobAttested path's
/// counter-carrying tail `(succeeded={s}, active={a})`, matching the
/// axis where operators grep for those counters. Both callsites now
/// route through the same primitive, so a future counter axis
/// (e.g. `startTime`, `completionTime`, `active` breakdown) lands at
/// ONE substrate primitive and every downstream evaluator picks up
/// the upgrade mechanically.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// three-invariant scaffold recurred at two Job-based evaluators
/// past the PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted
/// to ONE owner here). THEORY.md §II.1 invariant 5 (composition
/// preserves proofs — the two evaluators now compose structurally
/// through ONE primitive; a regression that drifted the wording at
/// ONE axis surfaces at [`classify_job_status_tests`] rather than as
/// silent drift at every future postcondition evaluator with a
/// Job-shaped substrate).
fn classify_job_status(
    label: &str,
    ns: &str,
    name: &str,
    lookup: JobLookup,
) -> Result<JobStatusView, Satisfaction> {
    match lookup {
        JobLookup::Missing => Err(Satisfaction::Unsatisfied(format!(
            "{label} {ns}/{name} not found"
        ))),
        JobLookup::Found(status) => {
            if status.failed > 0 {
                Err(Satisfaction::Unsatisfied(format!(
                    "{label} {ns}/{name} failed (status.failed={})",
                    status.failed
                )))
            } else if status.succeeded < 1 {
                Err(Satisfaction::Unsatisfied(format!(
                    "{label} {ns}/{name} still running (succeeded={}, active={})",
                    status.succeeded, status.active
                )))
            } else {
                Ok(status)
            }
        }
    }
}

/// Async wrapper: fetch a Job's status and project the three-invariant
/// scaffold through [`classify_job_status`]. Returns
/// `Ok(Ok(JobStatusView))` when the Job has succeeded (status.succeeded
/// ≥ 1 && status.failed == 0), `Ok(Err(Satisfaction::Unsatisfied(...)))`
/// on the Missing / failed / running short-circuits, and propagates
/// fetch-time errors as the outer `Err`.
async fn require_succeeded_job(
    client: Client,
    ns: &str,
    name: &str,
    label: &str,
) -> Result<Result<JobStatusView, Satisfaction>> {
    let lookup = fetch_job_status(client, ns, name).await?;
    Ok(classify_job_status(label, ns, name, lookup))
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
    let parsed: ProcessPhaseParams = match parse_condition_params("ProcessPhase", params) {
        Ok(p) => p,
        Err(unk) => return Ok(unk),
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
    let parsed: NamedResourceParams = match parse_condition_params(kind, params) {
        Ok(p) => p,
        Err(unk) => return Ok(unk),
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
        let env = ReceiptEnvelope::build(ReceiptKind::ClosedLoopAuth, "aaaa", "bbbb", "cccc", None);
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
        let env = ReceiptEnvelope::build(ReceiptKind::ClosedLoopAuth, "aaaa", "bbbb", "cccc", None);
        let payload = serde_json::to_string(&env).unwrap();
        let root = env.composed_root.clone();

        let v = parse_receipt_payload(&payload, Some(&root));
        assert!(matches!(v, ReceiptVerdict::Ok(_)));

        let v = parse_receipt_payload(&payload, Some("nope"));
        assert!(
            matches!(v, ReceiptVerdict::Malformed(ref m) if m.contains("composed_root mismatch"))
        );
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
        assert!(
            matches!(v, ReceiptVerdict::Malformed(ref m) if m.contains("version != tatara-receipt/v1"))
        );
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
        assert!(
            matches!(v, ReceiptVerdict::Malformed(ref m) if m.to_lowercase().contains("invalid"))
        );
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
        let env = ReceiptEnvelope::build(ReceiptKind::DbMigration, "aaaa", "bbbb", "cccc", None);
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

/// Substrate-primitive tests for [`parse_condition_params`] — the
/// shared `serde_json::from_value → Satisfaction::Unknown` axis every
/// kind-typed evaluator on this file dispatches through.
///
/// The pre-lift shape lived at FOUR sites across
/// [`evaluate_process_phase`] / [`evaluate_flux_ready`] /
/// [`evaluate_job_attested`] / [`evaluate_closed_loop_auth`], each
/// hand-authoring the (clone Value, from_value::<T>, wrap Err in
/// `Satisfaction::Unknown("<kind> params invalid: {e}")`) three-step
/// verbatim at its own local site. Post-lift the three shared invariants
/// live at ONE substrate primitive, and this module pins the four
/// contract axes at fail-before-pass-after granularity.
///
/// Sibling of the `tests` module above (on the [`Satisfaction`] /
/// [`phase_rank`] / [`parse_receipt_payload`] axes) — kept as its own
/// module so a regression at the substrate primitive's axis surfaces
/// distinctly from the per-kind evaluator or the receipt parser.
#[cfg(test)]
mod parse_condition_params_tests {
    use super::{parse_condition_params, JobAttestedParams, NamedResourceParams, Satisfaction};
    use serde_json::json;

    // ── Contract axis 1: successful deserialization returns Ok(T) ────
    //
    // The typed carrier's fields project structurally from the JSON;
    // the primitive is the identity path on the happy branch (a
    // wrapper only around the Err branch's Satisfaction promotion).

    #[test]
    fn valid_named_resource_params_deserialize_to_typed_carrier() {
        let params = json!({ "name": "my-kustomization", "namespace": "flux-system" });
        let parsed: NamedResourceParams = parse_condition_params("Kustomization", &params)
            .expect("valid params deserialize to typed carrier");
        assert_eq!(parsed.name, "my-kustomization");
        assert_eq!(parsed.namespace.as_deref(), Some("flux-system"));
    }

    #[test]
    fn valid_named_resource_params_with_missing_optional_namespace_deserialize() {
        // `NamedResourceParams::namespace` is `#[serde(default)] Option<String>`
        // — an absent field projects to `None` without rejecting.
        let params = json!({ "name": "only-name-required" });
        let parsed: NamedResourceParams = parse_condition_params("Kustomization", &params)
            .expect("valid params deserialize with optional field absent");
        assert_eq!(parsed.name, "only-name-required");
        assert_eq!(parsed.namespace, None);
    }

    #[test]
    fn valid_job_attested_params_deserialize_camelcase_fields() {
        // `JobAttestedParams` uses `#[serde(rename_all = "camelCase")]`,
        // so the JSON key `expectReceipt` deserializes into the Rust
        // field `expect_receipt`. Pinning this axis catches a regression
        // that swapped the primitive's `serde_json::from_value` for a
        // shape gate that lost the rename attribute (e.g. by round-
        // tripping through `serde_json::to_value` at the wrong axis).
        let params = json!({
            "name": "my-job",
            "namespace": "default",
            "expectReceipt": true,
            "receiptConfigMap": "my-cm",
        });
        let parsed: JobAttestedParams = parse_condition_params("JobAttested", &params)
            .expect("valid camelCase params deserialize into snake_case fields");
        assert_eq!(parsed.name, "my-job");
        assert!(parsed.expect_receipt);
        assert_eq!(parsed.receipt_config_map.as_deref(), Some("my-cm"));
    }

    // ── Contract axis 2: Err returns Satisfaction::Unknown (not
    // Unsatisfied or Satisfied) — pins the primitive's typed-projection
    // discipline so a future consumer that pattern-matches on the
    // Satisfaction variant binds structurally.

    #[test]
    fn invalid_params_project_to_satisfaction_unknown_variant() {
        // `NamedResourceParams::name` is a required `String`; passing
        // an integer produces a serde-level type-mismatch. `.err()`
        // projects `Result<T, E>` to `Option<E>` without requiring
        // `Debug` on the carrier `T`, then `.expect(...)` fires the
        // fail-branch message iff the primitive silently returned Ok.
        let params = json!({ "name": 42 });
        let err = parse_condition_params::<NamedResourceParams>("Kustomization", &params)
            .err()
            .expect("integer-typed name field must reject at serde gate");
        assert!(
            matches!(err, Satisfaction::Unknown(_)),
            "invalid params must project to Satisfaction::Unknown"
        );
    }

    #[test]
    fn absent_required_field_projects_to_satisfaction_unknown() {
        // `NamedResourceParams::name` is required (no serde default);
        // an empty object rejects at the serde-level missing-field gate.
        let params = json!({});
        let err = parse_condition_params::<NamedResourceParams>("Kustomization", &params)
            .err()
            .expect("missing required field must reject at serde gate");
        assert!(
            matches!(err, Satisfaction::Unknown(_)),
            "missing required field must project to Satisfaction::Unknown"
        );
    }

    // ── Contract axis 3: kind_label rides through as the diagnostic
    // prefix verbatim, ahead of " params invalid: " and the serde-
    // error tail. Pins the four workspace-shipped kind labels
    // ("ProcessPhase", "Kustomization" / "HelmRelease" via
    // evaluate_flux_ready's dynamic `kind` slot, "JobAttested",
    // "ClosedLoopAuth") so a regression that drifted the prefix
    // wording at ONE site fails here.

    #[test]
    fn kind_label_prefixes_the_unknown_diagnostic_across_all_shipped_axes() {
        for kind_label in [
            "ProcessPhase",
            "Kustomization",
            "HelmRelease",
            "JobAttested",
            "ClosedLoopAuth",
        ] {
            let params = json!({});
            let err = parse_condition_params::<NamedResourceParams>(kind_label, &params)
                .err()
                .expect("missing-field params must reject at serde gate");
            let Satisfaction::Unknown(msg) = err else {
                panic!("expected Satisfaction::Unknown for {kind_label}");
            };
            assert!(
                msg.starts_with(&format!("{kind_label} params invalid: ")),
                "kind label {kind_label:?} must prefix the diagnostic verbatim; \
                 got message {msg:?}"
            );
        }
    }

    #[test]
    fn kind_label_is_the_only_prefix_source_no_hardcoded_label_leaks() {
        // A regression that hardcoded a specific label in the
        // primitive (e.g. by copy-pasting "ProcessPhase params invalid"
        // as a literal instead of interpolating `{kind_label}`) would
        // surface here — the axis-typed prefix "AxisXYZ" is guaranteed
        // absent from the substrate's own trait paths.
        let params = json!({});
        let err = parse_condition_params::<NamedResourceParams>("AxisXYZ", &params)
            .err()
            .expect("missing-field params must reject at serde gate");
        let Satisfaction::Unknown(msg) = err else {
            panic!("expected Satisfaction::Unknown");
        };
        assert!(
            msg.starts_with("AxisXYZ params invalid: "),
            "kind label must interpolate through, got {msg:?}"
        );
    }

    // ── Contract axis 4: serde error's Display projection rides
    // through the trailing tail of the diagnostic. Pins that the
    // primitive does not drop or reshape the underlying serde error.

    #[test]
    fn serde_error_display_projects_through_the_diagnostic_tail() {
        let params = json!({ "name": 42 });
        let err = parse_condition_params::<NamedResourceParams>("Kustomization", &params)
            .err()
            .expect("integer-typed name field must reject at serde gate");
        let Satisfaction::Unknown(msg) = err else {
            panic!("expected Satisfaction::Unknown");
        };
        // serde_json's Display for an integer-vs-string mismatch names
        // the expected type ("string"); the substring is load-bearing
        // for operator grep-based diagnostics.
        let tail = msg
            .strip_prefix("Kustomization params invalid: ")
            .expect("primitive must emit the '<kind> params invalid: ' prefix verbatim");
        assert!(
            tail.contains("string"),
            "serde error tail must name the expected type verbatim; got {tail:?}"
        );
    }
}

/// Substrate-primitive tests for [`classify_job_status`] — the pure
/// (label, ns, name, JobLookup) → Result<JobStatusView, Satisfaction>
/// projection every Job-based postcondition evaluator on this file
/// dispatches through via [`require_succeeded_job`].
///
/// The pre-lift shape lived at TWO sites across
/// [`evaluate_job_attested`] and [`evaluate_closed_loop_auth`], each
/// hand-authoring the three-invariant scaffold (Missing / failed > 0
/// / succeeded < 1) verbatim at its own local site — differing only
/// in the label prefix each site threaded into the diagnostic. Post-
/// lift the three shared invariants live at ONE substrate primitive,
/// and this module pins the four contract axes (Missing, Failed,
/// Running, Succeeded) plus the label-riding, Satisfaction-variant,
/// and priority-ordering axes at fail-before-pass-after granularity.
///
/// Sibling of the [`parse_condition_params_tests`] module above (on
/// the shape-gate axis) and the `tests` module (on the
/// [`Satisfaction`] / [`phase_rank`] / [`parse_receipt_payload`]
/// axes) — kept as its own module so a regression at the primitive's
/// projection axis surfaces distinctly from the per-kind evaluator or
/// the shape gate.
#[cfg(test)]
mod classify_job_status_tests {
    use super::{classify_job_status, JobLookup, JobStatusView, Satisfaction};

    fn view(succeeded: i64, failed: i64, active: i64) -> JobStatusView {
        JobStatusView {
            succeeded,
            failed,
            active,
        }
    }

    // ── Contract axis 1: Succeeded projects to Ok(JobStatusView) ─────
    //
    // A Job with `status.succeeded ≥ 1 && status.failed == 0` returns
    // the fetched view verbatim so downstream evaluators (receipt
    // verification, ...) can continue with structural access to the
    // counter axes.

    #[test]
    fn succeeded_job_projects_to_ok_with_status_view() {
        let result = classify_job_status(
            "Job",
            "flux-system",
            "my-job",
            JobLookup::Found(view(1, 0, 0)),
        );
        let status = result.expect("succeeded Job must project to Ok(JobStatusView)");
        assert_eq!(status, view(1, 0, 0));
    }

    #[test]
    fn multi_succeeded_job_still_projects_to_ok() {
        // A completions=N Job may have succeeded > 1; the gate is
        // `succeeded >= 1`, so higher counts remain Ok.
        let result = classify_job_status(
            "Job",
            "default",
            "batch-job",
            JobLookup::Found(view(5, 0, 0)),
        );
        assert!(
            result.is_ok(),
            "succeeded > 1 must remain Ok, got {result:?}"
        );
    }

    // ── Contract axis 2: Missing projects to Unsatisfied verbatim ────
    //
    // Pins the exact diagnostic wording operators grep for; a
    // regression that reshaped the "not found" phrase surfaces here
    // rather than as silent operator-facing drift.

    #[test]
    fn missing_job_projects_to_unsatisfied_with_label_prefix() {
        let result = classify_job_status("Job", "flux-system", "my-job", JobLookup::Missing);
        let err = result.expect_err("Missing lookup must project to Err(Satisfaction)");
        assert!(
            matches!(err, Satisfaction::Unsatisfied(_)),
            "Missing must project to Satisfaction::Unsatisfied variant"
        );
        let Satisfaction::Unsatisfied(msg) = err else {
            unreachable!()
        };
        assert_eq!(msg, "Job flux-system/my-job not found");
    }

    // ── Contract axis 3: Failed projects to Unsatisfied verbatim ─────
    //
    // Pins the (status.failed={n}) counter tail — operators grep for
    // both the label prefix and the counter to alert on stuck Jobs.

    #[test]
    fn failed_job_projects_to_unsatisfied_with_counter_tail() {
        let result =
            classify_job_status("Job", "default", "my-job", JobLookup::Found(view(0, 3, 0)));
        let err = result.expect_err("failed > 0 must project to Err(Satisfaction)");
        let Satisfaction::Unsatisfied(msg) = err else {
            panic!("expected Satisfaction::Unsatisfied");
        };
        assert_eq!(msg, "Job default/my-job failed (status.failed=3)");
    }

    // ── Contract axis 4: Running projects to Unsatisfied verbatim ────
    //
    // Post-lift the closed-loop path also carries the `(succeeded={s},
    // active={a})` counter tail — pins that the more informative
    // wording rides through unconditionally, so a future evaluator on
    // the same primitive picks up the counters mechanically.

    #[test]
    fn running_job_projects_to_unsatisfied_with_counter_tail() {
        let result =
            classify_job_status("Job", "default", "my-job", JobLookup::Found(view(0, 0, 2)));
        let err = result.expect_err("succeeded < 1 must project to Err(Satisfaction)");
        let Satisfaction::Unsatisfied(msg) = err else {
            panic!("expected Satisfaction::Unsatisfied");
        };
        assert_eq!(
            msg,
            "Job default/my-job still running (succeeded=0, active=2)"
        );
    }

    // ── Contract axis 5: label prefix rides through both callsites ───
    //
    // Pins that both workspace-shipped labels (`"Job"` for
    // JobAttested, `"closed-loop probe Job"` for ClosedLoopAuth) ride
    // through the same primitive verbatim; a regression that
    // hardcoded a specific label at the primitive would surface at
    // both sub-axes here.

    #[test]
    fn label_prefixes_all_three_unsatisfied_projections_across_shipped_labels() {
        for label in ["Job", "closed-loop probe Job"] {
            // Missing
            let err = classify_job_status(label, "ns", "job", JobLookup::Missing)
                .expect_err("Missing must Err");
            let Satisfaction::Unsatisfied(msg) = err else {
                panic!("expected Unsatisfied for Missing at label={label:?}");
            };
            assert!(
                msg.starts_with(&format!("{label} ns/job")),
                "Missing diagnostic must start with '{label} ns/job'; got {msg:?}"
            );

            // Failed
            let err = classify_job_status(label, "ns", "job", JobLookup::Found(view(0, 1, 0)))
                .expect_err("failed > 0 must Err");
            let Satisfaction::Unsatisfied(msg) = err else {
                panic!("expected Unsatisfied for Failed at label={label:?}");
            };
            assert!(
                msg.starts_with(&format!("{label} ns/job failed")),
                "Failed diagnostic must start with '{label} ns/job failed'; got {msg:?}"
            );

            // Running
            let err = classify_job_status(label, "ns", "job", JobLookup::Found(view(0, 0, 1)))
                .expect_err("succeeded < 1 must Err");
            let Satisfaction::Unsatisfied(msg) = err else {
                panic!("expected Unsatisfied for Running at label={label:?}");
            };
            assert!(
                msg.starts_with(&format!("{label} ns/job still running")),
                "Running diagnostic must start with '{label} ns/job still running'; got {msg:?}"
            );
        }
    }

    // ── Contract axis 6: Failed takes priority over Running ──────────
    //
    // A Job with both `failed > 0` and `succeeded < 1` (partial
    // failure, some parallel pods failed but none succeeded yet)
    // must diagnose as Failed (the terminal condition), not Running
    // (the transient one). Pins the branch order at the primitive.

    #[test]
    fn failed_takes_priority_over_running_when_both_hold() {
        let result =
            classify_job_status("Job", "default", "my-job", JobLookup::Found(view(0, 1, 0)));
        let err = result.expect_err("failed > 0 must Err even when succeeded < 1");
        let Satisfaction::Unsatisfied(msg) = err else {
            panic!("expected Satisfaction::Unsatisfied");
        };
        assert!(
            msg.contains("failed"),
            "Failed must take branch priority over Running; got {msg:?}"
        );
        assert!(
            !msg.contains("still running"),
            "Failed branch must not fall through to Running wording; got {msg:?}"
        );
    }
}
