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
use crate::lifetime_clock::{self, AutoTerminate};
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
    let already_allocated = p.status.as_ref().and_then(|s| s.pid.clone()).is_some();
    if !already_allocated {
        let identity = p
            .status
            .as_ref()
            .and_then(|s| s.identity.clone())
            .unwrap_or_else(|| derive_identity(&p.spec, p.spec.identity.name_override.as_deref()));

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

        patch::patch_process_status(&api, &name, json!({ "pid": new_pid, "parent": parent_pid }))
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

    // 2. Intent variant dispatch — variants that the render module
    //    can synthesize K8s/Flux resources for proceed; the rest log
    //    and wait. Aplicacao landed in P2 (ephemeral env path).
    match p.spec.intent.variant()? {
        IntentVariant::Flux(_) | IntentVariant::Aplicacao(_) => {}
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

    // R9 — emit routing edges (Ingress + DNSEndpoint per declared
    // hostname) alongside the Intent-driven resources. The stable-
    // claim form is gated on the claim arbiter's decision; until
    // R10's controller loop lands, holds_stable_claim is computed
    // here as a placeholder: a Process holding the claim has its
    // pid/name stamped on `status.attestation.composed_root` already,
    // but the actual cluster-wide arbiter hasn't run yet. Default
    // false ⇒ instance-form only emits on first render.
    let routing_resources = if let Some(routing) = &p.spec.routing {
        let dns_lb = ctx.config.dns_lb_target.as_deref();
        let routes = render::render_routing(
            p,
            routing,
            false, // claim arbiter wires this in a follow-up — instance-form only for now
            &ctx.config.cluster,
            &ctx.config.location,
            &ctx.config.domain,
            dns_lb,
        )
        .map_err(|e| anyhow!("render routing: {e}"))?;
        for res in &routes {
            ssapply::apply_owned(ctx.kube.clone(), p, &ns, res.clone()).await?;
        }
        routes.len()
    } else {
        0
    };

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
        routing = routing_resources,
        "execing → running (RENDER)"
    );
    Ok(Action::requeue(Duration::from_secs(SHORT_RETRY)))
}

pub async fn handle_running(p: &Process, ctx: &Context) -> Result<Action> {
    // VERIFY — poll each owned Flux CR for Ready; update per-ref status;
    //          advance to Attested when all are Ready.
    let (ns, name) = namespace_and_name(p)?;

    // Ephemeral TTL clock — if the lifetime is :ephemeral and TTL has
    // elapsed, force-transition to Exiting regardless of postcondition
    // state. The phase machine handles SIGTERM cascade from there.
    if let AutoTerminate::Now { reason } =
        lifetime_clock::evaluate(p, ProcessPhase::Running, chrono::Utc::now())
    {
        return transition_to_exiting(ctx, &ns, &name, &reason).await;
    }

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
        let obj = ssapply::fetch(
            ctx.kube.clone(),
            &r.namespace,
            &r.api_version,
            &r.kind,
            &r.name,
        )
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
    // transition to Reconverging. Ephemeral lifetimes with a teardown
    // policy that includes Attested skip the heartbeat and SIGTERM now.
    let (ns, name) = namespace_and_name(p)?;

    if let AutoTerminate::Now { reason } =
        lifetime_clock::evaluate(p, ProcessPhase::Attested, chrono::Utc::now())
    {
        // Route through Releasing iff applicable exports declared.
        // Empty exports / no-trigger-match → fall through to the
        // existing Attested → Exiting path (zero-trace ephemeral).
        if has_applicable_exports(p, ProcessPhase::Attested) {
            return transition_to_releasing(ctx, &ns, &name, &reason).await;
        }
        return transition_to_exiting(ctx, &ns, &name, &reason).await;
    }

    let refs = p
        .status
        .as_ref()
        .map(|s| s.flux_resources.clone())
        .unwrap_or_default();

    let mut drift = false;
    for r in &refs {
        let obj = ssapply::fetch(
            ctx.kube.clone(),
            &r.namespace,
            &r.api_version,
            &r.kind,
            &r.name,
        )
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

/// Releasing — the export window. Process has reached a terminal
/// gate (`Attested` or `Failed`) and declared `ExportSpec`s that
/// match the gate; the reconciler emits one tatara-export-worker
/// Job per spec, watches them through their batch/v1 phase, and
/// advances the Process when every Job has reached a terminal
/// state.
///
/// Post-Releasing destination depends on which terminal-reached
/// gate the Process came through:
///   - Attested → Releasing → Exiting (cascade children, then Zombie)
///   - Failed   → Releasing → Zombie  (no cascade; resources already in error state)
///
/// The gate is recorded on the `tatara.pleme.io/released-from`
/// annotation by [`transition_to_releasing`]. Missing annotation
/// defaults to `Attested` for forward-compat with older Processes
/// that may pre-date the annotation contract.
pub async fn handle_releasing(p: &Process, ctx: &Context) -> Result<Action> {
    let (ns, name) = namespace_and_name(p)?;

    // 1. Recover the gate we came through from the annotation.
    let gate = released_from_annotation(p);

    // 2. Filter applicable exports for that gate. Nothing applicable
    //    = nothing to do; advance immediately to the post-Releasing
    //    destination. Operators see this in the logs as a "no-op
    //    Releasing" — useful when an export-less spec is mistakenly
    //    routed here by a race in transition_to_releasing.
    let applicable: Vec<(usize, &tatara_process::export::ExportSpec)> = p
        .spec
        .lifetime
        .ephemeral
        .iter()
        .flat_map(|e| {
            e.exports.iter().enumerate().filter(|(_, s)| match gate {
                ProcessPhase::Attested => s.when.fires_on_attested(),
                ProcessPhase::Failed => s.when.fires_on_failed(),
                _ => false,
            })
        })
        .collect();

    if applicable.is_empty() {
        return advance_out_of_releasing(ctx, &ns, &name, gate, "no applicable exports").await;
    }

    // 3. Render + apply (idempotent SSA) one Job per applicable
    //    export. The renderer is pure (render::render_export_jobs);
    //    apply_owned wires owner refs + std annotations.
    let rendered = render::render_export_jobs(
        p,
        gate,
        &ctx.config.export_worker_image,
        &ctx.config.export_worker_service_account,
    )
    .map_err(|e| anyhow!("render export jobs: {e}"))?;
    for job in rendered {
        ssapply::apply_owned(ctx.kube.clone(), p, &ns, job)
            .await
            .map_err(|e| anyhow!("apply export job: {e}"))?;
    }

    // 4. Watch all our export Jobs. Use a label selector that picks
    //    up only this Process's exports — not any sibling Process's.
    let jobs_api: Api<k8s_openapi::api::batch::v1::Job> =
        Api::namespaced(ctx.kube.clone(), &ns);
    let selector = format!(
        "{}={},{}=export",
        tatara_process::annotations::PROCESS,
        format!("{ns}/{name}"),
        tatara_process::annotations::ROLE,
    );
    let lp = kube::api::ListParams::default().labels(&selector);
    let jobs = jobs_api
        .list(&lp)
        .await
        .map_err(|e| anyhow!("list export jobs: {e}"))?;

    let mut total = 0usize;
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut active = 0usize;
    for j in &jobs.items {
        total += 1;
        let st = j.status.as_ref();
        if st.and_then(|s| s.succeeded).unwrap_or(0) > 0 {
            succeeded += 1;
        } else if st.and_then(|s| s.failed).unwrap_or(0) > 0 {
            failed += 1;
        } else {
            active += 1;
        }
    }

    info!(
        namespace = %ns,
        name = %name,
        gate = ?gate,
        applicable = applicable.len(),
        jobs_total = total,
        succeeded,
        failed,
        active,
        "releasing — export Jobs in flight"
    );

    if active > 0 || total < applicable.len() {
        // Some Jobs still running, or some Jobs not yet picked up by
        // our list (Job creation lag). Heartbeat back.
        return Ok(Action::requeue(Duration::from_secs(SHORT_RETRY)));
    }

    // All Jobs reached a terminal state. Even a Failed Job is fine —
    // the worker writes its receipt either way, and the FSM advances
    // both Attested-from and Failed-from paths regardless.
    advance_out_of_releasing(
        ctx,
        &ns,
        &name,
        gate,
        &format!("exports complete (succeeded={succeeded}, failed={failed})"),
    )
    .await
}

/// Inspect `tatara.pleme.io/released-from` to determine which
/// terminal-reached gate the Process came through. Defaults to
/// `Attested` when absent (forward-compat: pre-annotation Processes
/// in Releasing are treated as Attested-routed).
fn released_from_annotation(p: &Process) -> ProcessPhase {
    let v = p
        .metadata
        .annotations
        .as_ref()
        .and_then(|m| m.get(tatara_process::annotations::RELEASED_FROM))
        .cloned()
        .unwrap_or_default();
    match v.as_str() {
        "Failed" => ProcessPhase::Failed,
        _ => ProcessPhase::Attested,
    }
}

/// Patch the Process to its post-Releasing destination per the
/// gate it came through. Operator-visible message records why we
/// left the export window (timeout-free path; budget enforcement
/// lives in the upcoming shigoto migration).
async fn advance_out_of_releasing(
    ctx: &Context,
    ns: &str,
    name: &str,
    gate: ProcessPhase,
    reason: &str,
) -> Result<Action> {
    let next = match gate {
        ProcessPhase::Failed => ProcessPhase::Zombie,
        _ => ProcessPhase::Exiting,
    };
    let api: Api<Process> = Api::namespaced(ctx.kube.clone(), ns);
    let body = json!({
        "phase": next,
        "phaseSince": chrono::Utc::now(),
        "message": format!("releasing → {next} — {reason}"),
    });
    patch::patch_process_status(&api, name, body)
        .await
        .map_err(|e| anyhow!("patch (releasing→{next}): {e}"))?;
    info!(
        namespace = %ns,
        name = %name,
        gate = ?gate,
        next = ?next,
        reason = %reason,
        "releasing → next"
    );
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
    // Non-zero exit. Ephemeral lifetimes with teardown_on_failed transition
    // through Exiting (children drain) before reaching Zombie; permanent
    // and Never-teardown ephemeral Processes go straight to Zombie so the
    // operator can inspect the failure.
    let (ns, name) = namespace_and_name(p)?;
    let api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);

    if let AutoTerminate::Now { reason } =
        lifetime_clock::evaluate(p, ProcessPhase::Failed, chrono::Utc::now())
    {
        // Route through Releasing iff applicable post-mortem exports
        // declared. Without any, Failed → Zombie directly (no export
        // window to run).
        if has_applicable_exports(p, ProcessPhase::Failed) {
            return transition_to_releasing(ctx, &ns, &name, &reason).await;
        }
        // Phase.rs marks Failed → Zombie as the only legal next step
        // when no exports route through Releasing. Honor the FSM and
        // let the cascade happen at Zombie via the existing
        // finalizer-driven owner GC, while still recording the
        // teardown reason so the operator sees why cleanup happened
        // automatically.
        let body = json!({
            "phase": ProcessPhase::Zombie,
            "phaseSince": chrono::Utc::now(),
            "message": reason,
        });
        patch::patch_process_status(&api, &name, body)
            .await
            .map_err(|e| anyhow!("patch (failed→zombie, ephemeral teardown): {e}"))?;
        info!(namespace = %ns, name = %name, "failed → zombie (ephemeral teardown)");
        return Ok(Action::requeue(Duration::from_secs(TICK_RETRY)));
    }

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

/// True iff the Process has at least one ephemeral export whose
/// trigger fires for `phase`. Wraps the typed
/// [`tatara_process::lifetime::EphemeralLifetime::has_applicable_exports`]
/// helper so the reconciler reads through the same predicate the
/// pool-reconciler + caixa renderer will when they grow their own
/// export awareness.
fn has_applicable_exports(p: &Process, phase: ProcessPhase) -> bool {
    p.spec
        .lifetime
        .ephemeral
        .as_ref()
        .map(|e| e.has_applicable_exports(phase))
        .unwrap_or(false)
}

/// Transition Attested/Failed → Releasing with an operator-visible
/// reason. Stamps `tatara.pleme.io/released-from = {Attested|Failed}`
/// on the Process metadata so `handle_releasing` can recover the
/// gate it came through without rebuilding a phase-history table.
///
/// Two patches: one annotation patch (metadata) + one status patch
/// (phase). The annotation stamp is idempotent — re-applying the
/// same annotation is a no-op on the wire.
async fn transition_to_releasing(
    ctx: &Context,
    ns: &str,
    name: &str,
    reason: &str,
) -> Result<Action> {
    let api: Api<Process> = Api::namespaced(ctx.kube.clone(), ns);

    // 1. Stamp the released-from annotation — derived from the
    //    *current* phase, which is the gate we're leaving.
    let gate = p_current_phase_str(&api, name).await?;
    let annotation_patch = json!({
        "metadata": {
            "annotations": {
                tatara_process::annotations::RELEASED_FROM: gate,
            }
        }
    });
    let pp = kube::api::PatchParams::apply("tatara-reconciler").force();
    api.patch(
        name,
        &pp,
        &kube::api::Patch::Apply::<serde_json::Value>(annotation_patch),
    )
    .await
    .map_err(|e| anyhow!("annotate released-from: {e}"))?;

    // 2. Patch phase=Releasing with the operator-visible reason.
    let body = json!({
        "phase": ProcessPhase::Releasing,
        "phaseSince": chrono::Utc::now(),
        "message": format!("releasing exports — {reason}"),
    });
    patch::patch_process_status(&api, name, body)
        .await
        .map_err(|e| anyhow!("patch (→releasing): {e}"))?;
    info!(
        namespace = %ns,
        name = %name,
        gate = %gate,
        reason = %reason,
        "→ releasing (export window opens)"
    );
    Ok(Action::requeue(Duration::from_secs(TICK_RETRY)))
}

/// Read the Process's current phase as a string for the
/// released-from annotation. Only valid values reach here:
/// "Attested" or "Failed" (the two terminal-reached gates).
async fn p_current_phase_str(api: &Api<Process>, name: &str) -> Result<String> {
    let p = api
        .get_status(name)
        .await
        .map_err(|e| anyhow!("get status (released-from): {e}"))?;
    let phase = p
        .status
        .as_ref()
        .map(|s| s.phase)
        .unwrap_or(ProcessPhase::Attested);
    Ok(match phase {
        ProcessPhase::Failed => "Failed".to_string(),
        _ => "Attested".to_string(),
    })
}

/// Transition Running/Attested → Exiting with an operator-visible reason.
/// The existing `handle_exiting` cascade drains children + delegates
/// resource GC to K8s ownerReferences.
async fn transition_to_exiting(
    ctx: &Context,
    ns: &str,
    name: &str,
    reason: &str,
) -> Result<Action> {
    let api: Api<Process> = Api::namespaced(ctx.kube.clone(), ns);
    let body = json!({
        "phase": ProcessPhase::Exiting,
        "phaseSince": chrono::Utc::now(),
        "message": reason,
    });
    patch::patch_process_status(&api, name, body)
        .await
        .map_err(|e| anyhow!("patch (→exiting, ephemeral): {e}"))?;
    info!(
        namespace = %ns,
        name = %name,
        reason = %reason,
        "→ exiting (ephemeral lifetime clock)"
    );
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
