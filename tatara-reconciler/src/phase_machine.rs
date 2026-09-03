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
    let (ns, name) = p.owned_coordinates_or_err()?;
    // Declared name-override rides through the ONE substrate
    // `Process::declared_name_override` primitive, sibling to the
    // same-shape `handle_forking` rehydration branch that reads the
    // same slot through the same primitive. Pre-lift this site
    // spelled the projection as `p.spec.identity.name_override
    // .as_deref()` — a hand-authored `.as_deref()` chain repeated
    // at both sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    // threshold; post-lift the pair collapses onto ONE owner in
    // `tatara-process::crd`. The trim/empty-filter that dispatches
    // between the `name_override: true` (verbatim) and `false`
    // (content-hash) branches lives INSIDE `derive_identity`, NOT
    // at the borrow site.
    let identity = derive_identity(&p.spec, p.declared_name_override());

    let api = ctx.process_api(&ns);
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
    let (ns, name) = p.owned_coordinates_or_err()?;
    let api = ctx.process_api(&ns);

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

    // 2. Allocate PID if we don't already have one. The presence
    //    gate rides through the ONE substrate `Process::observed_pid`
    //    primitive, sibling to the same-corner `observed_flux_resources`
    //    call the VERIFY / ATTEST consumers dispatch through. Pre-lift
    //    this site spelled the gate as `.status.as_ref().and_then(|s|
    //    s.pid.clone()).is_some()` — a full `String` clone allocated
    //    and immediately dropped every reconcile pass through
    //    `handle_forking` even when the ALLOCATE-PID branch never
    //    reads the string; post-lift the borrow-form primitive drops
    //    the clone at the source.
    let already_allocated = p.observed_pid().is_some();
    if !already_allocated {
        // Borrow-form status-projection corner of the substrate
        // observed-* primitive family — pre-lift this was a hand-
        // authored `.status.as_ref().and_then(|s| s.identity.clone())`
        // chain past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
        // threshold, sibling to the `ssapply::inject_annotations`
        // content-hash annotation composer that walked the same
        // 3-line chain. Post-lift both sites route through the ONE
        // `Process::observed_identity` primitive; the FORK-time
        // seed clones off the borrow only at the composition point
        // where the owned-`Identity` fallback requires it (empty-
        // borrow corner: `Option::cloned` on `None` is `None`, so
        // the fallback fires without spending an intermediate
        // allocation). A future normalization (a content-hash
        // re-derivation gate, a stale-identity annotation, a
        // canonicalization pass rejecting a malformed name)
        // reaches this seed through the ONE primitive.
        let identity = p
            .observed_identity()
            .cloned()
            .unwrap_or_else(|| derive_identity(&p.spec, p.declared_name_override()));

        let pt_api = ctx.process_table_api();
        let pt = patch::ensure_process_table(&pt_api, &ctx.config.process_table_name)
            .await
            .map_err(|e| anyhow!("ensure ProcessTable: {e}"))?;
        let next_seq = pt.spec.next_sequence;
        // Declared parent PID rides through the ONE substrate
        // `Process::declared_parent_pid` primitive, sibling to the
        // same-shape `handle_exiting` child-fan-out filter that reads
        // each candidate child's declared parent through the same
        // primitive. Pre-lift this site spelled the projection as
        // `p.spec.identity.parent.as_deref()` — a hand-authored
        // `.as_deref()` chain repeated at both sites past the ★★
        // PRIME-DIRECTIVE ≥ 2 duplication threshold; post-lift the
        // pair collapses onto ONE owner in `tatara-process::crd`.
        let parent_pid = p.declared_parent_pid();
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
    patch::patch_process_status(
        &api,
        &name,
        patch::phase_status_msg(ProcessPhase::Execing, "dependencies satisfied"),
    )
    .await
    .map_err(|e| anyhow!("patch (forking→execing): {e}"))?;

    info!(namespace = %ns, name = %name, "forking → execing");
    Ok(Action::requeue(Duration::from_secs(TICK_RETRY)))
}

// The prior private `namespace_and_name(p)` helper lifted to the
// substrate as `Process::owned_coordinates_or_err(&self)` — see the
// method's doc for the owned + name-required peer of the coordinate-
// primitive family. Post-lift the 10 callsites in this module and the
// 2 in `crate::signals` route through ONE substrate owner (previously
// the private helper served phase_machine.rs only; signals.rs
// restated the same 2-slot unwrap chain by hand).

pub async fn handle_execing(p: &Process, ctx: &Context) -> Result<Action> {
    // 1. PROVE — evaluate `boundary.preconditions`; stay in Execing if unmet.
    // 2. RENDER — dispatch on intent variant; emit owned FluxCD CRs; advance to Running.
    let (ns, name) = p.owned_coordinates_or_err()?;

    // 1. Preconditions gate.
    let preconditions = &p.spec.boundary.preconditions;
    if !preconditions.is_empty() {
        let checked = evaluate_conditions(ctx.kube.clone(), p, preconditions).await?;
        let all_pass = checked.iter().all(|c| c.satisfied);
        let api = ctx.process_api(&ns);
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
    // hostname) alongside the Intent-driven resources.
    //
    // R10 wiring: holds_stable_claim is read from ProcessTable.status.
    // claims. If any claim record holder == "${ns}/${name}", this
    // Process holds the claim for at least one (cluster, app) tuple
    // and emits the stable-form Ingress + DNSEndpoint additionally.
    let routing_resources = if let Some(routing) = &p.spec.routing {
        let holds_stable = process_holds_any_claim(ctx, p).await;
        let dns_lb = ctx.config.dns_lb_target.as_deref();
        let routes = render::render_routing(
            p,
            routing,
            holds_stable,
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

    let api = ctx.process_api(&ns);
    patch::patch_process_status(
        &api,
        &name,
        patch::phase_status_with(ProcessPhase::Running, "fluxResources", &refs),
    )
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
    let (ns, name) = p.owned_coordinates_or_err()?;

    // Ephemeral TTL clock — if the lifetime is :ephemeral and TTL has
    // elapsed, force-transition to Exiting regardless of postcondition
    // state. The phase machine handles SIGTERM cascade from there.
    if let AutoTerminate::Now { reason } =
        lifetime_clock::evaluate(p, ProcessPhase::Running, chrono::Utc::now())
    {
        return transition_to_exiting(ctx, &ns, &name, &reason.to_string()).await;
    }

    // The 5-line `.status.as_ref().map(|s| s.flux_resources.clone())
    // .unwrap_or_default()` chain rides through the typed status-
    // projection primitive [`Process::observed_flux_resources`] —
    // sibling to `handle_attested`'s ATTEST-heartbeat consumer of
    // the same slice. Post-lift both consumers borrow the same
    // slice through ONE substrate method; the pre-lift `.clone()`
    // disappears because the enclosing async fn does not mutate
    // `p` past this line.
    let refs = p.observed_flux_resources();

    if refs.is_empty() {
        // Nothing was rendered — trivially proceed.
        return advance_to_attested(p, ctx, &ns, &name, None).await;
    }

    let mut updated: Vec<FluxResourceRef> = Vec::with_capacity(refs.len());
    let mut all_ready = true;
    for r in refs {
        let obj = ssapply::fetch_flux_ref(ctx.kube.clone(), r).await?;

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
        // 7-slot `FluxResourceRef { …, last_check: Some(chrono::
        // Utc::now()) }` struct-literal rides through the ONE
        // substrate composer [`FluxResourceRef::observed`], sibling
        // to the post-SSA `flux_ref_from_json` seeder that shares
        // the same composer. Pre-lift both sites restated the same
        // 7-slot struct-literal (four `String` coordinates + `ready`
        // + `message` + `last_check: Some(chrono::Utc::now())`) past
        // the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger; post-lift
        // the stamp lives at ONE substrate site so a future clock-
        // injection point lands there, not at every hand-authored
        // `Some(chrono::Utc::now())` stamp.
        updated.push(FluxResourceRef::observed(
            r.api_version.clone(),
            r.kind.clone(),
            r.name.clone(),
            r.namespace.clone(),
            ready,
            message,
        ));
    }

    // Always patch updated per-ref state so users see live progress.
    let api = ctx.process_api(&ns);
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
        let api = ctx.process_api(&ns);
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
    let (ns, name) = p.owned_coordinates_or_err()?;

    if let AutoTerminate::Now { reason } =
        lifetime_clock::evaluate(p, ProcessPhase::Attested, chrono::Utc::now())
    {
        // Route through Releasing iff applicable exports declared.
        // Empty exports / no-trigger-match → fall through to the
        // existing Attested → Exiting path (zero-trace ephemeral).
        let rendered = reason.to_string();
        if has_applicable_exports(p, ProcessPhase::Attested) {
            return transition_to_releasing(ctx, &ns, &name, &rendered).await;
        }
        return transition_to_exiting(ctx, &ns, &name, &rendered).await;
    }

    // Sibling to `handle_running`'s VERIFY-phase consumer; both ride
    // through the ONE substrate primitive [`Process::observed_flux_resources`]
    // so a future normalization (a per-ref staleness gate, an
    // owner-generation filter) reaches both callers mechanically.
    let refs = p.observed_flux_resources();

    let mut drift = false;
    for r in refs {
        let obj = ssapply::fetch_flux_ref(ctx.kube.clone(), r).await?;
        if !matches!(
            obj.as_ref().map(ssapply::ready_condition),
            Some(ssapply::ReadyState::Ready)
        ) {
            drift = true;
            break;
        }
    }

    if drift {
        let api = ctx.process_api(&ns);
        patch::patch_process_status(
            &api,
            &name,
            patch::phase_status_msg(ProcessPhase::Reconverging, "flux resource drift detected"),
        )
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

    // Borrow-form status-projection corner of the substrate
    // observed-* primitive family — pre-lift this was a hand-authored
    // `.status.as_ref().and_then(|s| s.attestation.as_ref())` chain
    // past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold, sibling
    // to the `render::render_export_jobs` export-Job builder that
    // walks the same 3-line chain to read the prior `composed_root`.
    // Post-lift both sites route through the ONE
    // `Process::observed_attestation` primitive — the missing-
    // `status` / empty-`attestation` gates land at ONE substrate
    // owner and a future normalization (verify-on-read, generation
    // filter, staleness gate) reaches both the ATTEST composer and
    // the export-Job builder through it.
    let next = match p.observed_attestation() {
        Some(prior) => prior.next(artifact_hash, control_hash, intent_hash),
        None => ProcessAttestation::initial(artifact_hash, control_hash, intent_hash),
    };

    let composed_root = next.composed_root.clone();
    let generation = next.generation;

    let api = ctx.process_api(ns);
    patch::patch_process_status(
        &api,
        name,
        patch::phase_status_with(ProcessPhase::Attested, "attestation", &next),
    )
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

/// Read the cluster-scoped ProcessTable + check if any entry in
/// `status.claims` points at this Process. Used by handle_execing
/// to gate stable-form Ingress emission. Failures (table missing,
/// API error) default to `false` — the claim arbiter will catch up
/// next ProcessTable reconcile.
async fn process_holds_any_claim(ctx: &Context, p: &Process) -> bool {
    // Borrow + name-required corner of the substrate coordinate-
    // primitive family — pre-lift this was a hand-authored
    // `.metadata.namespace.as_deref().unwrap_or("default")` +
    // `.metadata.name.as_deref().unwrap_or("")` + `is_empty` early-
    // return chain past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    // threshold, sibling to the child-fan-out chain in
    // `handle_exiting` below (which spelled the name gate as
    // `unwrap_or_default()` + a silent empty-name `child_api.delete("")`
    // no-op). Post-lift both sites route through the ONE
    // `Process::coordinates_or_none` primitive — the name gate lands
    // at ONE substrate owner and a future normalization (case-fold,
    // unicode-safe collation, cross-cluster prefix) reaches every
    // borrow + name-required consumer through it.
    let Some((ns, name)) = p.coordinates_or_none() else {
        return false;
    };
    let process_ref = crate::ssapply::qualified_process_ref(ns, name);
    let table_api = ctx.process_table_api();
    let table = match table_api.get(&ctx.config.process_table_name).await {
        Ok(t) => t,
        Err(_) => return false,
    };
    let claims = match &table.status {
        Some(s) => &s.claims,
        None => return false,
    };
    claims.values().any(|c| c.holder == process_ref)
}

/// Stable hash of the Intent — canonical serde JSON → BLAKE3.
///
/// Delegates the 2-link `hex::encode(blake3::hash(bytes).as_bytes())`
/// step to the substrate primitive [`tatara_process::hash::hex_blake3`]
/// so this Intent-shaped pillar is byte-identical to the peer render
/// pillar producers ([`crate::render::artifact_hash`],
/// [`crate::render::intent_hash`]) that also route through it.
fn compute_intent_hash(intent: &tatara_process::intent::Intent) -> String {
    let bytes = serde_json::to_vec(intent).unwrap_or_default();
    tatara_process::hash::hex_blake3(&bytes)
}

/// Build a `FluxResourceRef` from the emitted JSON — post-apply initial state.
///
/// Delegates the 4-slot coordinate extraction to
/// [`RenderedResourceCoords::from_json`] — the substrate primitive
/// that owns the shared `.get(K).and_then(|v| v.as_str()).ok_or_else(
/// || anyhow!("rendered resource missing X"))?.to_string()` walk
/// (see also [`crate::ssapply::apply_owned`], the sibling consumer).
fn flux_ref_from_json(res: &Value) -> Result<FluxResourceRef> {
    let coords = RenderedResourceCoords::from_json(res)?;
    let namespace = coords.namespace_or_default().to_string();
    // 7-slot `FluxResourceRef { …, last_check: Some(chrono::Utc
    // ::now()) }` struct-literal rides through the ONE substrate
    // composer [`FluxResourceRef::observed`], sibling to the
    // VERIFY-phase `handle_running` per-ref rebuild that shares
    // the same composer. Post-SSA the ref is stamped `ready =
    // false` with the canonical `"applied; awaiting reconciliation"`
    // wording that the pre-lift hand-authored literal spelled
    // verbatim here.
    Ok(FluxResourceRef::observed(
        coords.api_version,
        coords.kind,
        coords.name,
        namespace,
        false,
        Some("applied; awaiting reconciliation".into()),
    ))
}

pub async fn handle_reconverging(p: &Process, ctx: &Context) -> Result<Action> {
    // SIGHUP or drift detected — flip back to Execing.
    let (ns, name) = p.owned_coordinates_or_err()?;
    let api = ctx.process_api(&ns);
    patch::patch_process_status(
        &api,
        &name,
        patch::phase_status(ProcessPhase::Execing, None),
    )
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
    let (ns, name) = p.owned_coordinates_or_err()?;

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
            e.exports
                .iter()
                .enumerate()
                .filter(|(_, s)| s.when.fires_on(gate))
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
    let jobs_api: Api<k8s_openapi::api::batch::v1::Job> = Api::namespaced(ctx.kube.clone(), &ns);
    let selector = format!(
        "{}={},{}=export",
        tatara_process::annotations::PROCESS,
        crate::ssapply::qualified_process_ref(&ns, &name),
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
    // Annotation lookup via the substrate primitive — pre-lift this
    // was a hand-authored 3-line `.metadata.annotations.as_ref()
    // .and_then(|m| m.get(RELEASED_FROM)).cloned().unwrap_or_default()`
    // chain, one of THREE workspace-wide restatements past the ★★
    // PRIME-DIRECTIVE ≥ 2 duplication threshold (peers at
    // `signals::ingest` and `tatara-pool-reconciler::controller_pool::
    // process_belongs_to_pool`). Post-lift the three lookups route
    // through ONE substrate owner [`Process::annotation`]; this
    // callsite matches directly on the returned `Option<&str>`
    // (`Some("Failed")` → `Failed`, everything else including the
    // pre-lift bare `""` fallback → `Attested`), preserving the
    // pre-lift dispatch behavior byte-for-byte across both corners
    // (annotation present with unexpected value, annotation absent).
    match p.annotation(tatara_process::annotations::RELEASED_FROM) {
        Some("Failed") => ProcessPhase::Failed,
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
    let api = ctx.process_api(ns);
    patch::patch_process_status(
        &api,
        name,
        patch::phase_status_msg(next, format!("releasing → {next} — {reason}")),
    )
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
    let (ns, name) = p.owned_coordinates_or_err()?;
    // SIGTERM cascade comparator seed: read the PID this Process
    // currently owns through the ONE substrate `Process::observed_pid`
    // primitive, peer to the sibling `handle_forking` ALLOCATE-PID
    // gate that routes through the same primitive. Pre-lift this
    // site spelled the projection as `.status.as_ref().and_then(|s|
    // s.pid.clone())` — an owned `Option<String>` immediately
    // re-borrowed through `.as_str()` before the comparator; post-
    // lift the borrow-form primitive yields the `&str` the
    // comparator wants directly.
    let my_pid = p.observed_pid();

    if let Some(pid) = my_pid {
        // Enumerate Processes cluster-wide and find direct children.
        let all = ctx.processes_all_api();
        let list = all
            .list(&kube::api::ListParams::default())
            .await
            .map_err(|e| anyhow!("list processes: {e}"))?;
        // Each candidate child's declared parent-PID rides through the
        // ONE substrate `Process::declared_parent_pid` primitive,
        // sibling to the same-shape `handle_forking` ALLOCATE-PID
        // composer that reads its own declared parent through the same
        // primitive. Pre-lift this filter spelled the projection as
        // `.spec.identity.parent.as_deref() == Some(pid)` — a hand-
        // authored `.as_deref()` chain repeated at both sites past
        // the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold; post-lift
        // the pair collapses onto ONE owner in `tatara-process::crd`.
        // The filter runs per candidate child across the cluster-wide
        // Process list; the borrow-form primitive avoids allocating
        // one `String` clone per non-matching row.
        let children: Vec<_> = list
            .items
            .into_iter()
            .filter(|c| c.declared_parent_pid() == Some(pid))
            .collect();
        if !children.is_empty() {
            for child in &children {
                // Skip ones already being deleted or missing a name —
                // the tombstone-presence gate rides through the ONE
                // substrate `Process::is_being_deleted` primitive
                // (sibling to the same-corner SIGTERM preempt in
                // `controller::reconcile`), and the name gate rides
                // through the ONE substrate `Process::coordinates_or_none`
                // primitive (sibling to the same-corner
                // `process_holds_any_claim` call above). Pre-lift the
                // name gate spelled as `.metadata.name.as_deref()
                // .unwrap_or_default()` followed by an implicit no-op
                // `child_api.delete("")` that silently 4xx'd against
                // the K8s API and discarded the error; post-lift the
                // `None` corner is a cleaner explicit `continue`.
                if child.is_being_deleted() {
                    continue;
                }
                let Some((cns, cname)) = child.coordinates_or_none() else {
                    continue;
                };
                let child_api = ctx.process_api(cns);
                // `Api::delete(name, &DeleteParams::default())` routes
                // through the ONE substrate primitive
                // `tatara_process::delete::default` — pre-lift this
                // was one of SEVEN workspace-wide hand-authored
                // restatements of the 2-link chain past the ★★ PRIME-
                // DIRECTIVE ≥ 2 duplication threshold. Post-lift every
                // DELETE-verb consumer (pool decision + convergence-
                // action arms, this reconciler SIGTERM cascade,
                // watcher PR-close arm) shares ONE substrate owner
                // alongside the peer create / patch primitives already
                // lifted in `tatara_process::{create,patch}`.
                let _ = tatara_process::delete::default(&child_api, cname).await;
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
    let api = ctx.process_api(&ns);
    patch::patch_process_status(&api, &name, patch::phase_status(ProcessPhase::Zombie, None))
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
    let (ns, name) = p.owned_coordinates_or_err()?;
    let api = ctx.process_api(&ns);

    if let AutoTerminate::Now { reason } =
        lifetime_clock::evaluate(p, ProcessPhase::Failed, chrono::Utc::now())
    {
        // Route through Releasing iff applicable post-mortem exports
        // declared. Without any, Failed → Zombie directly (no export
        // window to run).
        let rendered = reason.to_string();
        if has_applicable_exports(p, ProcessPhase::Failed) {
            return transition_to_releasing(ctx, &ns, &name, &rendered).await;
        }
        // Phase.rs marks Failed → Zombie as the only legal next step
        // when no exports route through Releasing. Honor the FSM and
        // let the cascade happen at Zombie via the existing
        // finalizer-driven owner GC, while still recording the
        // teardown reason so the operator sees why cleanup happened
        // automatically.
        patch::patch_process_status(
            &api,
            &name,
            patch::phase_status_msg(ProcessPhase::Zombie, rendered),
        )
        .await
        .map_err(|e| anyhow!("patch (failed→zombie, ephemeral teardown): {e}"))?;
        info!(namespace = %ns, name = %name, "failed → zombie (ephemeral teardown)");
        return Ok(Action::requeue(Duration::from_secs(TICK_RETRY)));
    }

    patch::patch_process_status(&api, &name, patch::phase_status(ProcessPhase::Zombie, None))
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
    let api = ctx.process_api(ns);

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
    // SSA `PatchParams` rides through the ONE substrate primitive
    // `ssapply::apply_patch_params` — pre-lift this slot was a
    // hand-authored `PatchParams::apply("tatara-reconciler").force()`
    // chain that spelled the field-manager string as a literal and
    // silently bypassed the `ssapply::FIELD_MANAGER` const, one of
    // THREE workspace-wide restatements past the ★★ PRIME-DIRECTIVE
    // ≥ 2 duplication threshold (peers at `ssapply::apply_owned` +
    // `table_controller::reconcile`). Post-lift a rename of
    // `FIELD_MANAGER` propagates through every SSA writer
    // mechanically — the hand-authored literal cannot reach the wire.
    let pp = ssapply::apply_patch_params();
    api.patch(
        name,
        &pp,
        &kube::api::Patch::Apply::<serde_json::Value>(annotation_patch),
    )
    .await
    .map_err(|e| anyhow!("annotate released-from: {e}"))?;

    // 2. Patch phase=Releasing with the operator-visible reason.
    patch::patch_process_status(
        &api,
        name,
        patch::phase_status_msg(
            ProcessPhase::Releasing,
            format!("releasing exports — {reason}"),
        ),
    )
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
    let phase = p.observed_phase().unwrap_or(ProcessPhase::Attested);
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
    let api = ctx.process_api(ns);
    patch::patch_process_status(
        &api,
        name,
        patch::phase_status_msg(ProcessPhase::Exiting, reason),
    )
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
    let (ns, name) = p.owned_coordinates_or_err()?;
    let api = ctx.process_api(&ns);
    patch::patch_process_status(&api, &name, patch::phase_status(ProcessPhase::Reaped, None))
        .await
        .map_err(|e| anyhow!("patch (zombie→reaped): {e}"))?;
    info!(namespace = %ns, name = %name, "zombie → reaped");
    Ok(Action::requeue(Duration::from_secs(TICK_RETRY)))
}

pub async fn handle_reaped(p: &Process, ctx: &Context) -> Result<Action> {
    // Release the finalizer — K8s GC removes the Process object + owned Flux CRs.
    let (ns, name) = p.owned_coordinates_or_err()?;
    let api = ctx.process_api(&ns);
    patch::remove_finalizer(&api, &name, p, tatara_process::PROCESS_FINALIZER)
        .await
        .map_err(|e| anyhow!("release finalizer: {e}"))?;
    info!(namespace = %ns, name = %name, "reaped — finalizer released");
    Ok(Action::await_change())
}
