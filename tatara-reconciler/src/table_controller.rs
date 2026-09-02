//! ProcessTable reconciler — `/proc` singleton that aggregates Process
//! status, reaps orphans/zombies, hands out PIDs, AND **arbitrates
//! the stable-name claim registry** (R10 controller wiring).
//!
//! Stable-name claim flow (one reconcile cycle):
//!
//!   1. List every Process cluster-wide.
//!   2. Filter to Processes with `spec.routing.stable_name_claim ==
//!      true` (the candidate set).
//!   3. Group by `${cluster}/${app}` — the claim registry's key.
//!   4. For each (cluster, app) key, build typed `Candidate`s + call
//!      `claim::decide_claim_for`. Result is one of:
//!        Hold   → no-op
//!        Transfer { next: ClaimRecord } → write into status.claims[key]
//!        Vacate → remove status.claims[key]
//!   5. PATCH `ProcessTable.status.claims` with the merged decision.
//!
//! The decision logic itself is pure (see `claim::decide_claim_for`);
//! this controller is the thin async glue that fetches Processes +
//! applies the decision via kube-rs.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use kube::api::{ListParams, Patch, PatchParams};
use kube::runtime::controller::Action;
use serde_json::json;
use tracing::{info, warn};

use tatara_process::prelude::*;
use tatara_process::table::ClaimRecord;

use crate::claim::{decide_claim_for, Candidate, ClaimDecision};
use crate::context::Context;

pub async fn reconcile(
    _table: Arc<ProcessTable>,
    ctx: Arc<Context>,
) -> Result<Action, kube::Error> {
    // (1) List every Process cluster-wide.
    let process_api = ctx.processes_all_api();
    let processes = process_api.list(&ListParams::default()).await?.items;

    // (2) + (3) Filter + group: build candidates per (cluster, app).
    // The (cluster, app) key uses the reconciler-config cluster as
    // the fallback when a hostname doesn't override.
    let cfg_cluster = ctx.config.cluster.as_str();
    let mut groups: BTreeMap<String, Vec<Candidate<'_>>> = BTreeMap::new();
    for p in &processes {
        // Process must declare a routing block with stable_name_claim.
        let routing = match p.spec.routing.as_ref() {
            Some(r) if r.stable_name_claim => r,
            _ => continue,
        };
        // Route the observed-phase pull through the ONE substrate
        // primitive `Process::observed_phase_or_pending` — pre-lift
        // this was a hand-authored `.observed_phase().unwrap_or
        // (ProcessPhase::Pending)` two-link chain, one of FOUR
        // workspace-wide restatements past the ★★ PRIME-DIRECTIVE
        // ≥ 2 duplication threshold (peers at `controller::reconcile`
        // and `boundary::evaluate_process_phase`, plus
        // `tatara-pool-reconciler::controller_pool::reconcile_pool`).
        // Post-lift the four consumers share ONE substrate owner
        // and this claim-arbiter row seed inherits any future
        // generation-filter or staleness-gate normalization
        // mechanically.
        let phase = p.observed_phase_or_pending();
        let pid = p
            .status
            .as_ref()
            .and_then(|s| s.pid.clone())
            .unwrap_or_default();
        // Creation-anchor probe rides through the ONE substrate
        // `Process::created_at` primitive (sibling to the TTL-expiry
        // gate + requeue-budget picker in `tatara-process::
        // lifetime_clock`); the `.unwrap_or_else(Utc::now)` tail keeps
        // the "just created" fallback the pre-lift chain applied when
        // the API server has not yet stamped the timestamp on a
        // freshly-forked Process.
        let created_at = p.created_at().unwrap_or_else(Utc::now);
        // Claim-row key rides through the substrate primitive
        // `Process::qualified_ref` — pre-lift this was a hand-authored
        // 2-step `coordinates_or_defaults() → qualified_process_ref(ns,
        // name)` composition that walked the SAME paired-projection +
        // shape-composer chain as `render::render_routing` and
        // `render::render_export_jobs` (three workspace-wide
        // restatements past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
        // threshold). Post-lift all three sites share ONE substrate
        // owner; the composed reference feeds
        // `ClaimRecord.holder` (the slot every downstream claim-registry
        // query greps by shape) byte-identically to the pre-lift
        // paired composition, so a rename of either fallback string,
        // a swap of the `<ns>/<name>` separator, or a future
        // `<ns>/<name>@<gen>` / `<cluster>/<ns>/<name>` extension lands
        // at `Process::qualified_ref` and the claim-arbiter row seed
        // inherits the upgrade mechanically.
        let process_ref = p.qualified_ref();
        for hostname in &routing.hostnames {
            // Cluster override rides through the ONE substrate
            // primitive `RoutingHostname::cluster_or` — pre-lift this
            // was a hand-authored `.cluster.as_deref().unwrap_or
            // (cfg_cluster)` chain, one of TWO workspace-wide
            // restatements past the ★★ PRIME-DIRECTIVE ≥ 2
            // duplication threshold (peer at
            // `render::render_routing`). Post-lift both consumers
            // share ONE substrate owner; a future normalization
            // (case-fold, cross-cluster alias resolution, per-fleet
            // canonicalization) lands at `RoutingHostname::cluster_or`
            // and every consumer inherits it mechanically.
            let cluster = hostname.cluster_or(cfg_cluster);
            let key = format!("{cluster}/{}", hostname.app);
            groups.entry(key).or_default().push(Candidate {
                process_ref: process_ref.clone(),
                pid: pid.clone(),
                priority: routing.priority,
                phase,
                created_at,
                _process: p,
            });
        }
    }

    // (4) Read current claims registry — singleton ProcessTable.
    let table_api = ctx.process_table_api();
    let current_table = table_api.get(&ctx.config.process_table_name).await.ok();
    let current_claims = current_table
        .as_ref()
        .and_then(|t| t.status.as_ref())
        .map(|s| s.claims.clone())
        .unwrap_or_default();

    // (5) Decide per key + accumulate the merged registry.
    let now = Utc::now();
    let mut new_claims: BTreeMap<String, ClaimRecord> = BTreeMap::new();
    let mut transfers: u32 = 0;
    let mut vacates: u32 = 0;
    let mut holds: u32 = 0;

    // Iterate over every key that has either a current claim OR a
    // candidate — Vacate is computed implicitly by absence in
    // new_claims, but we count it for telemetry.
    let mut all_keys: BTreeMap<String, ()> = BTreeMap::new();
    for k in current_claims.keys() {
        all_keys.insert(k.clone(), ());
    }
    for k in groups.keys() {
        all_keys.insert(k.clone(), ());
    }

    for key in all_keys.keys() {
        let current = current_claims.get(key);
        let candidates: &[Candidate<'_>] = groups.get(key).map(|v| v.as_slice()).unwrap_or(&[]);
        match decide_claim_for(key, current, candidates, now) {
            ClaimDecision::Hold => {
                holds += 1;
                if let Some(c) = current {
                    new_claims.insert(key.clone(), c.clone());
                }
            }
            ClaimDecision::Transfer { next } => {
                transfers += 1;
                info!(
                    key = %key,
                    holder = %next.holder,
                    priority = next.priority,
                    "claim transferred"
                );
                new_claims.insert(key.clone(), next);
            }
            ClaimDecision::Vacate => {
                vacates += 1;
                if current.is_some() {
                    info!(key = %key, "claim vacated (no live candidates)");
                }
                // omit from new_claims
            }
        }
    }

    // (6) PATCH status.claims if anything changed. Idempotent —
    // re-applying the same map is a no-op on the wire.
    if new_claims != current_claims {
        let patch = json!({ "status": { "claims": new_claims } });
        let pp = PatchParams::apply("tatara-reconciler").force();
        if let Err(e) = table_api
            .patch_status(&ctx.config.process_table_name, &pp, &Patch::Apply(&patch))
            .await
        {
            warn!(error = %e, "patch ProcessTable.status.claims failed; will retry");
        }
    }

    info!(
        processes = processes.len(),
        candidates_total = groups.values().map(Vec::len).sum::<usize>(),
        keys = all_keys.len(),
        holds,
        transfers,
        vacates,
        "ProcessTable heartbeat"
    );

    Ok(Action::requeue(Duration::from_secs(30)))
}

pub fn error_policy(_t: Arc<ProcessTable>, err: &kube::Error, _ctx: Arc<Context>) -> Action {
    tracing::warn!(error = %err, "ProcessTable reconcile error; requeuing");
    Action::requeue(Duration::from_secs(30))
}
