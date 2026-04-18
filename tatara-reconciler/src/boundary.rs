//! Boundary condition evaluator — the VERIFY half of the convergence loop.
//!
//! Evaluates the six `ConditionKind` variants against a live cluster state:
//! - `ProcessPhase`:           lookup the referenced Process, compare phase
//! - `KustomizationHealthy`:   fetch the Kustomization, read `status.conditions[Ready]`
//! - `HelmReleaseReleased`:    same, for `HelmRelease`
//! - `PromQL`:                 stub (returns Unknown) — needs a metrics client
//! - `Cel`:                    stub (returns Unknown) — needs a CEL runtime
//! - `NixEval`:                stub (returns Unknown) — needs tatara-engine
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
    let default_ns = process
        .metadata
        .namespace
        .as_deref()
        .unwrap_or("default");
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
        ProcessPhase::Exiting
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
    let default_ns = process
        .metadata
        .namespace
        .as_deref()
        .unwrap_or("default");
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
                        message: format!(
                            "{}/{} is {actual_phase}; need {required}",
                            ns, dep.name
                        ),
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
        assert!(!phase_reached(ProcessPhase::Running, ProcessPhase::Attested));
        assert!(!phase_reached(ProcessPhase::Pending, ProcessPhase::Running));
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
        assert_eq!(Satisfaction::Unsatisfied("why".into()).message(), Some("why"));
        assert_eq!(Satisfaction::Satisfied.message(), None);
    }
}
