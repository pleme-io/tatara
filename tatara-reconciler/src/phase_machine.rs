//! Per-phase handlers — the 8-phase universal convergence loop, mapped onto
//! Unix process lifecycle.
//!
//! | Phase        | Universal loop step      | What happens                                   |
//! |--------------|--------------------------|------------------------------------------------|
//! | Pending      | DECLARE                  | canonicalize spec, compute content hash        |
//! | Forking      | DECLARE + PID assign     | register in ProcessTable, link parent          |
//! | Execing      | SIMULATE + PROVE + RENDER| evaluate intent, emit FluxCD CRs                |
//! | Running      | DEPLOY + VERIFY (pre)    | wait for Flux resources ready, check pre-conds |
//! | Attested     | VERIFY (post) + ATTEST   | check post-conds, compose three-pillar hash    |
//! | Reconverging | RECONVERGE               | re-enter Execing (drift / SIGHUP)              |
//! | Exiting      | (terminate)              | drain children, clean owned CRs                |
//! | Failed       | (terminate)              | exit code set, awaiting Zombie→Reaped          |
//! | Zombie       | (terminate)              | children gone, waiting on finalizer            |
//! | Reaped       | (GC)                     | finalizer released                             |

use std::time::Duration;

use anyhow::{anyhow, Result};
use kube::runtime::controller::Action;
use kube::{Api, Client};
use serde_json::{json, Value};
use tracing::{info, warn};

use tatara_process::boundary::Condition;
use tatara_process::identity::derive_identity;
use tatara_process::intent::IntentVariant;
use tatara_process::prelude::*;
use tatara_process::status::CheckedCondition;

use crate::context::Context;
use crate::{boundary, patch, pid, render, ssapply};

const HEARTBEAT: u64 = 30;
const SHORT_RETRY: u64 = 5;
const TICK_RETRY: u64 = 1;

pub async fn handle_pending(p: &Process, ctx: &Context) -> Result<Action> {
    // DECLARE — canonicalize the spec, compute content hash, attach Identity,
    //           install the tatara finalizer, advance to Forking.
    let (ns, name) = namespace_and_name(p)?;
    let identity = derive_identity(&p.spec, p.spec.identity.name_override.as_deref());

    let api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);
    patch::ensure_finalizer(&api, &name, p, tatara_process::PROCESS_FINALIZER)
        .await
        .map_err(|e| anyhow!("install finalizer: {e}"))?;

    let patch_body = patch::phase_status(ProcessPhase::Forking, Some(&identity));
    patch::patch_process_status(&api, &name, patch_body)
        .await
        .map_err(|e| anyhow!("patch status: {e}"))?;

    info!(
        namespace = %ns,
        name = %name,
        identity_name = %identity.name,
        content_hash = %identity.content_hash,
        "pending → forking (DECLARE)"
    );
    Ok(Action::requeue(Duration::from_secs(TICK_RETRY)))
}

pub async fn handle_forking(p: &Process, ctx: &Context) -> Result<Action> {
    // 1. Check `dependsOn` — stay in Forking if any dep unmet.
    // 2. Allocate PID from `ProcessTable.nextSequence` (idempotent if already set).
    // 3. Advance to Execing.
    let (ns, name) = namespace_and_name(p)?;
    let api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);

    // 1. Dependency gate.
    let unmet = boundary::check_depends_on(ctx.kube.clone(), p)
        .await
        .map_err(|e| anyhow!("depends_on check: {e}"))?;
    if !unmet.is_empty() {
        let messages: Vec<String> = unmet.iter().map(|u| u.message.clone()).collect();
        let body = json!({
            "message": format!(
                "waiting on {} dependency/dependencies: {}",
                unmet.len(),
                messages.join("; ")
            ),
        });
        // Best-effort — if the patch fails we'll just retry.
        let _ = patch::patch_process_status(&api, &name, body).await;
        info!(
            namespace = %ns,
            name = %name,
            unmet = unmet.len(),
            "forking — dependencies unmet; will retry"
        );
        return Ok(Action::requeue(Duration::from_secs(HEARTBEAT)));
    }

    // 2. Allocate PID if we don't already have one.
    let already_allocated = p
        .status
        .as_ref()
        .and_then(|s| s.pid.clone())
        .is_some();
    if !already_allocated {
        let identity = p
            .status
            .as_ref()
            .and_then(|s| s.identity.clone())
            .unwrap_or_else(|| {
                derive_identity(&p.spec, p.spec.identity.name_override.as_deref())
            });

        let pt_api: Api<ProcessTable> = Api::all(ctx.kube.clone());
        let pt = patch::ensure_process_table(&pt_api, &ctx.config.process_table_name)
            .await
            .map_err(|e| anyhow!("ensure ProcessTable: {e}"))?;
        let next_seq = pt.spec.next_sequence;
        let parent_pid = p.spec.identity.parent.as_deref();
        let new_pid = pid::allocate_pid(&identity, parent_pid, next_seq);

        patch::patch_process_table_spec(
            &pt_api,
            &ctx.config.process_table_name,
            json!({ "nextSequence": next_seq + 1 }),
        )
        .await
        .map_err(|e| anyhow!("bump nextSequence: {e}"))?;

        patch::patch_process_status(
            &api,
            &name,
            json!({ "pid": new_pid, "parent": parent_pid }),
        )
        .await
        .map_err(|e| anyhow!("patch pid: {e}"))?;

        info!(
            namespace = %ns,
            name = %name,
            pid = %new_pid,
            parent = ?parent_pid,
            "PID assigned"
        );
    }

    // 3. Advance to Execing.
    let body = json!({
        "phase": ProcessPhase::Execing,
        "phaseSince": chrono::Utc::now(),
        "message": "dependencies satisfied",
    });
    patch::patch_process_status(&api, &name, body)
        .await
        .map_err(|e| anyhow!("patch (forking→execing): {e}"))?;

    info!(namespace = %ns, name = %name, "forking → execing");
    Ok(Action::requeue(Duration::from_secs(TICK_RETRY)))
}

fn namespace_and_name(p: &Process) -> Result<(String, String)> {
    let ns = p
        .metadata
        .namespace
        .clone()
        .unwrap_or_else(|| "default".into());
    let name = p
        .metadata
        .name
        .clone()
        .ok_or_else(|| anyhow!("Process has no metadata.name"))?;
    Ok((ns, name))
}

pub async fn handle_execing(p: &Process, ctx: &Context) -> Result<Action> {
    // 1. PROVE — evaluate `boundary.preconditions`; stay in Execing if unmet.
    // 2. RENDER — dispatch on intent variant; emit owned FluxCD CRs; advance to Running.
    let (ns, name) = namespace_and_name(p)?;

    // 1. Preconditions gate.
    let preconditions = &p.spec.boundary.preconditions;
    if !preconditions.is_empty() {
        let checked = evaluate_conditions(ctx.kube.clone(), p, preconditions).await?;
        let all_pass = checked.iter().all(|c| c.satisfied);
        let api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);
        let body = json!({
            "boundary": { "preconditions": checked },
            "message": if all_pass {
                "preconditions satisfied".to_string()
            } else {
                "waiting on preconditions".to_string()
            },
        });
        let _ = patch::patch_process_status(&api, &name, body).await;
        if !all_pass {
            info!(
                namespace = %ns,
                name = %name,
                checked = checked.len(),
                "execing — preconditions unmet"
            );
            return Ok(Action::requeue(Duration::from_secs(HEARTBEAT)));
        }
    }

    // 2. Intent variant dispatch.
    match p.spec.intent.variant()? {
        IntentVariant::Flux(_) => {}
        other => {
            warn!(
                namespace = %ns,
                name = %name,
                variant = ?std::mem::discriminant(&other),
                "execing — intent variant not yet implemented, staying in Execing"
            );
            return Ok(Action::requeue(Duration::from_secs(HEARTBEAT)));
        }
    }

    let output = render::render(p, &p.spec.intent)?;
    let mut refs: Vec<FluxResourceRef> = Vec::with_capacity(output.resources.len());
    for res in &output.resources {
        ssapply::apply_owned(ctx.kube.clone(), p, &ns, res.clone()).await?;
        refs.push(flux_ref_from_json(res)?);
    }

    let api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);
    let body = json!({
        "phase": ProcessPhase::Running,
        "phaseSince": chrono::Utc::now(),
        "fluxResources": refs,
    });
    patch::patch_process_status(&api, &name, body)
        .await
        .map_err(|e| anyhow!("patch status (execing→running): {e}"))?;

    info!(
        namespace = %ns,
        name = %name,
        resources = refs.len(),
        "execing → running (RENDER)"
    );
    Ok(Action::requeue(Duration::from_secs(SHORT_RETRY)))
}

pub async fn handle_running(p: &Process, ctx: &Context) -> Result<Action> {
    // VERIFY — poll each owned Flux CR for Ready; update per-ref status;
    //          advance to Attested when all are Ready.
    let (ns, name) = namespace_and_name(p)?;
    let refs = p
        .status
        .as_ref()
        .map(|s| s.flux_resources.clone())
        .unwrap_or_default();

    if refs.is_empty() {
        // Nothing was rendered — trivially proceed.
        return advance_to_attested(p, ctx, &ns, &name, None).await;
    }

    let mut updated: Vec<FluxResourceRef> = Vec::with_capacity(refs.len());
    let mut all_ready = true;
    for r in &refs {
        let obj =
            ssapply::fetch(ctx.kube.clone(), &r.namespace, &r.api_version, &r.kind, &r.name)
                .await
                .map_err(|e| anyhow!("fetch {}/{}: {e}", r.kind, r.name))?;

        let (ready, message) = match obj.as_ref().map(ssapply::ready_condition) {
            Some(ssapply::ReadyState::Ready) => (true, None),
            Some(ssapply::ReadyState::NotReady(m)) => {
                all_ready = false;
                (false, m)
            }
            Some(ssapply::ReadyState::Unknown) | None => {
                all_ready = false;
                (false, Some("not yet observed".to_string()))
            }
        };
        updated.push(FluxResourceRef {
            api_version: r.api_version.clone(),
            kind: r.kind.clone(),
            name: r.name.clone(),
            namespace: r.namespace.clone(),
            ready,
            message,
            last_check: Some(chrono::Utc::now()),
        });
    }

    // Always patch updated per-ref state so users see live progress.
    let api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);
    patch::patch_process_status(&api, &name, json!({ "fluxResources": updated }))
        .await
        .map_err(|e| anyhow!("patch fluxResources: {e}"))?;

    if !all_ready {
        info!(namespace = %ns, name = %name, "running (VERIFY — not all flux refs ready)");
        return Ok(Action::requeue(Duration::from_secs(HEARTBEAT)));
    }

    // All flux refs ready — now evaluate boundary.postconditions.
    let postconditions = &p.spec.boundary.postconditions;
    if !postconditions.is_empty() {
        let checked = evaluate_conditions(ctx.kube.clone(), p, postconditions).await?;
        let all_pass = checked.iter().all(|c| c.satisfied);
        let api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);
        patch::patch_process_status(
            &api,
            &name,
            json!({ "boundary": { "postconditions": checked } }),
        )
        .await
        .map_err(|e| anyhow!("patch postconditions: {e}"))?;
        if !all_pass {
            info!(
                namespace = %ns,
                name = %name,
                checked = checked.len(),
                "running — postconditions unmet"
            );
            return Ok(Action::requeue(Duration::from_secs(HEARTBEAT)));
        }
    }

    // Derive artifact_hash from the observed resource identities.
    let mut h = blake3::Hasher::new();
    for r in &updated {
        h.update(r.api_version.as_bytes());
        h.update(b"/");
        h.update(r.kind.as_bytes());
        h.update(b"/");
        h.update(r.namespace.as_bytes());
        h.update(b"/");
        h.update(r.name.as_bytes());
        h.update(b"\n");
    }
    let artifact_hash = hex::encode(h.finalize().as_bytes());
    advance_to_attested(p, ctx, &ns, &name, Some(artifact_hash)).await
}

/// Evaluate each `Condition` against the cluster and build status rows.
async fn evaluate_conditions(
    client: Client,
    process: &Process,
    conditions: &[Condition],
) -> Result<Vec<CheckedCondition>> {
    let mut out = Vec::with_capacity(conditions.len());
    for c in conditions {
        let sat = boundary::evaluate(client.clone(), process, c)
            .await
            .map_err(|e| anyhow!("evaluate {:?}: {e}", c.kind))?;
        out.push(CheckedCondition {
            condition: c.clone(),
            satisfied: sat.is_satisfied(),
            last_check: Some(chrono::Utc::now()),
            message: sat.message().map(String::from),
        });
    }
    Ok(out)
}

pub async fn handle_attested(p: &Process, ctx: &Context) -> Result<Action> {
    // ATTEST heartbeat — re-check Flux resources; if any drift to NotReady,
    // transition to Reconverging. Otherwise stay Attested.
    let (ns, name) = namespace_and_name(p)?;
    let refs = p
        .status
        .as_ref()
        .map(|s| s.flux_resources.clone())
        .unwrap_or_default();

    let mut drift = false;
    for r in &refs {
        let obj =
            ssapply::fetch(ctx.kube.clone(), &r.namespace, &r.api_version, &r.kind, &r.name)
                .await
                .map_err(|e| anyhow!("fetch {}/{}: {e}", r.kind, r.name))?;
        if !matches!(
            obj.as_ref().map(ssapply::ready_condition),
            Some(ssapply::ReadyState::Ready)
        ) {
            drift = true;
            break;
        }
    }

    if drift {
        let api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);
        let body = json!({
            "phase": ProcessPhase::Reconverging,
            "phaseSince": chrono::Utc::now(),
            "message": "flux resource drift detected",
        });
        patch::patch_process_status(&api, &name, body)
            .await
            .map_err(|e| anyhow!("patch (attested→reconverging): {e}"))?;
        info!(namespace = %ns, name = %name, "attested → reconverging (DRIFT)");
        Ok(Action::requeue(Duration::from_secs(SHORT_RETRY)))
    } else {
        info!(namespace = %ns, name = %name, "attested (heartbeat)");
        Ok(Action::requeue(Duration::from_secs(HEARTBEAT)))
    }
}

/// Compute pillars + compose the next attestation + patch status.
async fn advance_to_attested(
    p: &Process,
    ctx: &Context,
    ns: &str,
    name: &str,
    artifact_hash: Option<String>,
) -> Result<Action> {
    let artifact_hash = artifact_hash.unwrap_or_default();
    let intent_hash = compute_intent_hash(&p.spec.intent);
    let control_hash: Option<String> = None; // compliance eval lands next

    let next = match p.status.as_ref().and_then(|s| s.attestation.as_ref()) {
        Some(prior) => prior.next(artifact_hash, control_hash, intent_hash),
        None => ProcessAttestation::initial(artifact_hash, control_hash, intent_hash),
    };

    let composed_root = next.composed_root.clone();
    let generation = next.generation;

    let api: Api<Process> = Api::namespaced(ctx.kube.clone(), ns);
    let body = json!({
        "phase": ProcessPhase::Attested,
        "phaseSince": chrono::Utc::now(),
        "attestation": next,
    });
    patch::patch_process_status(&api, name, body)
        .await
        .map_err(|e| anyhow!("patch attestation: {e}"))?;

    info!(
        namespace = %ns,
        name = %name,
        generation,
        root = %composed_root,
        "running → attested (ATTEST)"
    );
    Ok(Action::requeue(Duration::from_secs(HEARTBEAT)))
}

/// Stable hash of the Intent — canonical serde JSON → BLAKE3.
fn compute_intent_hash(intent: &tatara_process::intent::Intent) -> String {
    let bytes = serde_json::to_vec(intent).unwrap_or_default();
    hex::encode(blake3::hash(&bytes).as_bytes())
}

/// Build a `FluxResourceRef` from the emitted JSON — post-apply initial state.
fn flux_ref_from_json(res: &Value) -> Result<FluxResourceRef> {
    let api_version = res
        .get("apiVersion")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("rendered resource missing apiVersion"))?
        .to_string();
    let kind = res
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("rendered resource missing kind"))?
        .to_string();
    let name = res
        .get("metadata")
        .and_then(|m| m.get("name"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("rendered resource missing metadata.name"))?
        .to_string();
    let namespace = res
        .get("metadata")
        .and_then(|m| m.get("namespace"))
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_string();
    Ok(FluxResourceRef {
        api_version,
        kind,
        name,
        namespace,
        ready: false,
        message: Some("applied; awaiting reconciliation".into()),
        last_check: Some(chrono::Utc::now()),
    })
}

pub async fn handle_reconverging(p: &Process, ctx: &Context) -> Result<Action> {
    // SIGHUP or drift detected — flip back to Execing.
    let (ns, name) = namespace_and_name(p)?;
    let api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);
    let body = json!({
        "phase": ProcessPhase::Execing,
        "phaseSince": chrono::Utc::now(),
    });
    patch::patch_process_status(&api, &name, body)
        .await
        .map_err(|e| anyhow!("patch (reconverging→execing): {e}"))?;
    info!(namespace = %ns, name = %name, "reconverging → execing (RECONVERGE)");
    Ok(Action::requeue(Duration::from_secs(TICK_RETRY)))
}

pub async fn handle_exiting(p: &Process, ctx: &Context) -> Result<Action> {
    // Cascade terminate: delete child Processes first, then move to Zombie.
    // Owner references on owned Flux CRs cause K8s to GC them once we're gone.
    let (ns, name) = namespace_and_name(p)?;
    let my_pid = p.status.as_ref().and_then(|s| s.pid.clone());

    if let Some(pid) = &my_pid {
        // Enumerate Processes cluster-wide and find direct children.
        let all: Api<Process> = Api::all(ctx.kube.clone());
        let list = all
            .list(&kube::api::ListParams::default())
            .await
            .map_err(|e| anyhow!("list processes: {e}"))?;
        let children: Vec<_> = list
            .items
            .into_iter()
            .filter(|c| c.spec.identity.parent.as_deref() == Some(pid.as_str()))
            .collect();
        if !children.is_empty() {
            for child in &children {
                let cns = child.metadata.namespace.as_deref().unwrap_or("default");
                let cname = child.metadata.name.as_deref().unwrap_or_default();
                // Skip ones already being deleted.
                if child.metadata.deletion_timestamp.is_some() {
                    continue;
                }
                let child_api: Api<Process> = Api::namespaced(ctx.kube.clone(), cns);
                let _ = child_api
                    .delete(cname, &kube::api::DeleteParams::default())
                    .await;
            }
            info!(
                namespace = %ns,
                name = %name,
                children = children.len(),
                "exiting — waiting for children to terminate"
            );
            return Ok(Action::requeue(Duration::from_secs(SHORT_RETRY)));
        }
    }

    // No children (or no pid — never forked). Advance to Zombie.
    let api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);
    let body = json!({
        "phase": ProcessPhase::Zombie,
        "phaseSince": chrono::Utc::now(),
    });
    patch::patch_process_status(&api, &name, body)
        .await
        .map_err(|e| anyhow!("patch (exiting→zombie): {e}"))?;
    info!(namespace = %ns, name = %name, "exiting → zombie");
    Ok(Action::requeue(Duration::from_secs(TICK_RETRY)))
}

pub async fn handle_failed(p: &Process, ctx: &Context) -> Result<Action> {
    // Non-zero exit — record and advance to Zombie.
    let (ns, name) = namespace_and_name(p)?;
    let api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);
    let body = json!({
        "phase": ProcessPhase::Zombie,
        "phaseSince": chrono::Utc::now(),
    });
    patch::patch_process_status(&api, &name, body)
        .await
        .map_err(|e| anyhow!("patch (failed→zombie): {e}"))?;
    info!(namespace = %ns, name = %name, "failed → zombie");
    Ok(Action::requeue(Duration::from_secs(TICK_RETRY)))
}

pub async fn handle_zombie(p: &Process, ctx: &Context) -> Result<Action> {
    // Final post-exit pass — advance to Reaped; the ProcessTable controller
    // may force-reap earlier on zombie_timeout_seconds overflow (future).
    let (ns, name) = namespace_and_name(p)?;
    let api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);
    let body = json!({
        "phase": ProcessPhase::Reaped,
        "phaseSince": chrono::Utc::now(),
    });
    patch::patch_process_status(&api, &name, body)
        .await
        .map_err(|e| anyhow!("patch (zombie→reaped): {e}"))?;
    info!(namespace = %ns, name = %name, "zombie → reaped");
    Ok(Action::requeue(Duration::from_secs(TICK_RETRY)))
}

pub async fn handle_reaped(p: &Process, ctx: &Context) -> Result<Action> {
    // Release the finalizer — K8s GC removes the Process object + owned Flux CRs.
    let (ns, name) = namespace_and_name(p)?;
    let api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);
    patch::remove_finalizer(&api, &name, p, tatara_process::PROCESS_FINALIZER)
        .await
        .map_err(|e| anyhow!("release finalizer: {e}"))?;
    info!(namespace = %ns, name = %name, "reaped — finalizer released");
    Ok(Action::await_change())
}
