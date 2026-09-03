//! Pool controller — applies `PoolDecision` to the cluster via kube-rs.
//!
//! The decision logic is pure (see `pool_decide`). This module is the
//! thin async glue: it fetches Pools + their owned Processes, calls
//! `decide_pool_reconcile`, and applies the result via kube-rs
//! create/delete + status patch.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use chrono::Utc;
use kube::api::Api;
use kube::runtime::controller::Action;
use tracing::{info, warn};

use crate::ReconcilerError;

use tatara_process::annotations;
use tatara_process::ephemeral::EphemeralSpec;
use tatara_process::lifetime::{Lifetime, PermanentLifetime};
use tatara_process::pool::{EphemeralPool, MemberState, PoolMember, PoolPhase, PoolStatus};
use tatara_process::prelude::{NamespacedApiCoordinates, Process, ProcessSpec};

use crate::context::PoolContext;
use crate::desired::{decide_pool_convergence, ConvergenceAction, PoolMemberSnapshot};
use crate::naming::member_process_name;
use crate::pool_decide::{decide_pool_reconcile, PoolDecision};

const POOL_FINALIZER: &str = "tatara.pleme.io/pool-finalizer";

/// One reconcile pass over a Pool. The kube-rs `Controller` calls this.
pub async fn reconcile(
    pool: Arc<EphemeralPool>,
    ctx: Arc<PoolContext>,
) -> std::result::Result<Action, ReconcilerError> {
    reconcile_inner(pool, ctx).await.map_err(Into::into)
}

async fn reconcile_inner(pool: Arc<EphemeralPool>, ctx: Arc<PoolContext>) -> Result<Action> {
    // The (namespace, name) API-path pair rides through the substrate
    // trait `tatara_process::NamespacedApiCoordinates` (blanket-implemented
    // over every `kube::Resource<DynamicType = ()>` in the workspace) —
    // pre-lift this was a hand-authored paired 5-line `.metadata.<slot>
    // .clone().ok_or_else(|| anyhow!("Pool has no metadata.<slot>"))?`
    // chain, one of TWO workspace-wide restatements past the ★★ PRIME-
    // DIRECTIVE ≥ 2 duplication threshold (peer at
    // `controller_allocation::reconcile_inner`'s Allocation top-level
    // gate; both funneled every downstream `Api::namespaced` +
    // `Api::patch` call through the same shape). Post-lift the two
    // reconcilers share ONE substrate owner + every future CRD in a
    // peer crate inherits the extractor for free at its own
    // `reconcile_inner` dispatcher; the error prefix now spells the
    // canonical kube kind (`EphemeralPool`) rather than the pre-lift
    // short-form (`Pool`), matching `kubectl get ephemeralpools`
    // output verbatim.
    let (ns, name) = pool.owned_coordinates_required()?;

    // Both `Api<EphemeralPool>` + `Api<Process>` handles ride through
    // the substrate primitives `PoolContext::pool_api` +
    // `PoolContext::process_api` — pre-lift both slots were hand-
    // authored `Api::namespaced(ctx.kube.clone(), &ns)` chains, one of
    // TWO workspace-wide restatements per typed collection past the
    // ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold (peers at
    // `controller_allocation::reconcile_inner`'s pool_api + process_api
    // seeds; both funneled every downstream `list` / `create` /
    // `delete` / `patch_status` call through the SAME `(client, ns)`
    // pair). Post-lift the two consumers per typed collection share
    // ONE substrate owner; a future change that layers request tracing
    // spans, a default `PatchParams` builder, a namespace-scoped
    // access-control gate, or per-request metrics onto every Api-typed
    // request lands at ONE site in `PoolContext` rather than being
    // restated at each callsite.
    let pool_api = ctx.pool_api(&ns);
    let process_api = ctx.process_api(&ns);

    // 1. Fetch the Processes owned by this Pool (annotation-matched).
    // Wire-verb dispatch routes through the ONE substrate primitive
    // `tatara_process::list::default` — pre-lift this was a hand-
    // authored `.list(&ListParams::default())` 2-link chain, one of
    // FOUR workspace-wide restatements past the ★★ PRIME-DIRECTIVE
    // ≥ 2 duplication threshold (peers at the claim-arbiter walk in
    // `tatara-reconciler::table_controller`, the SIGTERM-cascade
    // fan-out in `tatara-reconciler::phase_machine`, and the
    // allocation controller's pool lookup in `controller_allocation`).
    // Post-lift the four consumers share ONE substrate owner; the
    // namespace-wide owned-members walk here inherits any future
    // paginated `limit` / reconciler-budget `timeout` / resource-
    // version-continuation normalization mechanically.
    let all_processes = tatara_process::list::default(&process_api)
        .await
        .map_err(|e| anyhow!("list Processes in {ns}: {e}"))?;
    let mut members: Vec<PoolMember> = Vec::new();
    let mut owned: Vec<Process> = Vec::new();
    for p in all_processes.items {
        if process_belongs_to_pool(&p, &name) {
            let state = process_to_member_state(&p);
            members.push(PoolMember {
                // Owned-form name projection rides through the ONE
                // substrate primitive `Process::owned_name_or_empty` —
                // pre-lift this was a hand-authored `.metadata.name
                // .clone().unwrap_or_default()` chain, one of TWO
                // workspace-wide restatements past the ★★ PRIME-
                // DIRECTIVE ≥ 2 duplication threshold (peer at the
                // desired-count `PoolMemberSnapshot` seed below).
                // Post-lift both consumers share ONE substrate owner;
                // a future name-canonicalization pass or per-pool
                // alias table lands at `tatara_process::prelude::
                // Process::owned_name_or_empty` and both row-builder
                // seeds inherit it mechanically.
                process_name: p.owned_name_or_empty(),
                state,
                // The last-transition anchor rides through the ONE
                // substrate primitive `Process::observed_phase_since`
                // — pre-lift this was a hand-authored 5-line
                // `.status.as_ref().and_then(|s| s.phase_since)`
                // chain, closing the LAST raw `.status.as_ref()`
                // chain in this reconciler's production code on
                // `Process`. Post-lift the chain rides ONE substrate
                // owner symmetric to the peer `p.created_at()
                // .unwrap_or_else(Utc::now)` seed the desired-count
                // `PoolMemberSnapshot` builder two branches below
                // already routes through — both timestamp-projection
                // primitives on `Process` (metadata-timestamp
                // `created_at` + status-timestamp
                // `observed_phase_since`) now compose byte-uniformly
                // at the pool reconciler's per-member row builder,
                // with the `.unwrap_or_else(Utc::now)` sink kept at
                // the callsite (matching every peer `observed_*`
                // primitive's pure-projection discipline). A future
                // normalization (a per-cluster clock-skew guard, a
                // staleness gate that treats a `phase_since`
                // predating a reconcile deadline as unobserved, a
                // canonicalization that folds a suspiciously-zero
                // slot to `None`) lands at ONE substrate site and
                // both this per-member seed AND any future
                // observed-transition consumer inherit the upgrade
                // mechanically.
                entered_state_at: p.observed_phase_since().unwrap_or_else(Utc::now),
                allocation_ref: None,
            });
            owned.push(p);
        }
    }

    // 2a. **R11 — desired-count loop gate.** When the operator
    //     declares `spec.desired > 0`, the pool seeks a stable
    //     healthy count and the legacy allocation-driven decision
    //     is bypassed. Snapshots come from observed Process
    //     phases (Running/Attested = healthy; Failed/Zombie/Reaped
    //     = failed; everything else = transient).
    if pool.spec.desired > 0 {
        // Snapshot each owned Process's (phase, created_at) pair through
        // the substrate primitive family on `Process` — pre-lift the
        // 3-line `.status.as_ref().map(|s| s.phase)` chain and the
        // 5-line `.metadata.creation_timestamp.as_ref().map(|t| t.0)
        // .unwrap_or_else(Utc::now)` chain were hand-authored here,
        // duplicating the lift already living at
        // [`tatara_process::prelude::Process::observed_phase`] (owner of
        // the copy-form status-projection primitive on the phase axis)
        // and [`tatara_process::prelude::Process::created_at`] (owner
        // of the copy-form metadata-projection primitive on the
        // creation-timestamp axis). Routing through both substrate
        // primitives means a future normalization (generation filter,
        // clock-skew guard, stale-observation gate) lands at ONE
        // substrate site and this pool-desired-count seed inherits it
        // mechanically — no per-callsite hand-edit here alongside the
        // sibling consumers in `tatara-reconciler` /
        // `tatara-process::lifetime_clock`.
        let snapshots: Vec<PoolMemberSnapshot> = owned
            .iter()
            .map(|p| PoolMemberSnapshot {
                // Owned-form name projection rides through the ONE
                // substrate primitive `Process::owned_name_or_empty` —
                // pre-lift this was a hand-authored `.metadata.name
                // .clone().unwrap_or_default()` chain, sibling to the
                // pool-member seed above; both routed through the
                // same substrate owner so any future name-canonicalization
                // pass lands once.
                process_name: p.owned_name_or_empty(),
                // Phase snapshot rides through the ONE substrate
                // primitive `Process::observed_phase_or_pending` —
                // pre-lift this was a hand-authored `.observed_phase()
                // .unwrap_or(ProcessPhase::Pending)` two-link chain,
                // one of FOUR workspace-wide restatements past the
                // ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold
                // (peers at `tatara-reconciler::controller::reconcile`
                // / `::boundary::evaluate_process_phase` /
                // `::table_controller::stable_name_group_key`).
                // Post-lift the four consumers share ONE substrate
                // owner and this desired-count seed inherits any
                // future generation-filter or staleness-gate
                // normalization mechanically.
                phase: p.observed_phase_or_pending(),
                // Creation-anchor probe rides through the ONE substrate
                // composer `Process::created_at_or` (pure wrapper over the
                // sibling `Process::created_at` projection that folds the
                // `.unwrap_or(fallback)` sink into ONE substrate owner) —
                // pre-lift this was a hand-authored 2-step
                // `.created_at().unwrap_or_else(Utc::now)` chain, one of
                // TWO production restatements past the ★★ PRIME-
                // DIRECTIVE ≥ 2 duplication threshold (peer at
                // `tatara-reconciler::table_controller::reconcile_process_table`'s
                // per-Process claim-row `created_at` seed; both stamped
                // the SAME wall-clock fallback on the same missing-
                // `metadata.creationTimestamp` corner). Post-lift the
                // two consumers share ONE substrate owner; the wall-
                // clock read stays at this callsite (as `Utc::now()`
                // passed positionally) so the composer itself stays
                // pure, matching the discipline every peer `observed_*`
                // accessor follows.
                created_at: p.created_at_or(Utc::now()),
            })
            .collect();
        let actions = decide_pool_convergence(&pool, &snapshots, Utc::now());
        info!(
            namespace = %ns,
            pool = %name,
            desired = pool.spec.desired,
            actions = actions.len(),
            "pool desired-count loop"
        );
        apply_convergence_actions(&pool, &process_api, &ns, &name, &members, actions).await?;

        // Update status from the same observations + skip the
        // legacy decision step.
        //
        // The 11-line `PoolStatus { phase, phase_since,
        // ready/allocated/spawning/returning counts (each a per-slot
        // `count_state` fanout), members, message: None, conditions:
        // vec![] }` seed rides through the substrate constructor
        // `PoolStatus::observed` — pre-lift this was hand-authored at
        // TWO sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
        // threshold in this module (peer at the legacy allocation-
        // driven `desired == 0` status-patch site below). Post-lift
        // both consumers share ONE substrate owner; the four counters
        // now ride a SINGLE closed-set-driven fold over the members
        // list rather than four independent filter-and-count passes,
        // and a future counter slot lands at ONE match arm in
        // `tatara_process::pool::PoolMember::state_count_fanout`.
        let phase = pool_phase_from_members(&pool, &members);
        let _ = tatara_process::patch::merge_status(
            &pool_api,
            &name,
            &PoolStatus::observed(phase, members.clone(), Utc::now()),
        )
        .await;
        return Ok(tatara_process::requeue::after_secs(
            ctx.config.heartbeat_seconds,
        ));
    }

    // 2b. Legacy allocation-driven decision (desired == 0 path).
    let decision = decide_pool_reconcile(&pool, &members, Utc::now());

    info!(
        namespace = %ns,
        pool = %name,
        members = members.len(),
        decision = ?decision,
        "pool reconcile"
    );

    // 3. Apply the decision.
    match decision {
        PoolDecision::NoOp => {}
        PoolDecision::Spawn { count } => {
            // The paired `(metadata.uid, metadata.name)` slot-slug
            // seed rides through the substrate primitive
            // `EphemeralPool::owned_uid_or_name_or_empty` — pre-lift
            // this was a hand-authored `.metadata.uid.clone()
            // .unwrap_or_else(|| name.<into>())` chain, one of TWO
            // in-file restatements past the ★★ PRIME-DIRECTIVE ≥ 2
            // duplication threshold (peer at `apply_convergence_
            // actions` in this same module; both feed the SAME
            // `member_process_name(&pool_name, &pool_uid_seed, slot)`
            // composer). Post-lift both consumers share ONE substrate
            // owner; a future normalization step (per-cluster prefix
            // stripper, case-fold key builder, hash-mixing pass)
            // lands at ONE substrate method rather than being restated
            // at each callsite.
            let pool_uid = pool.owned_uid_or_name_or_empty();
            // The `HashSet<String>` occupied-names collision-set seed
            // rides through the substrate primitive
            // [`tatara_process::pool::PoolMember::process_names_set`] —
            // pre-lift this was a hand-authored
            // `members.iter().map(|m| m.process_name.clone()).collect
            // ::<HashSet<_>>()` chain, one of TWO in-file restatements
            // past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold
            // (peer at `apply_convergence_actions` below; both feed
            // the SAME `.contains(&member_process_name(&name,
            // &pool_uid, slot))` collision probe per spawn slot).
            // Post-lift both consumers share ONE substrate owner; a
            // future normalization step on the occupied-name axis
            // (case-fold before insertion, per-cluster prefix strip,
            // exclusion of Returning/Failed members that no longer
            // own their slot) lands at ONE substrate method rather
            // than at each callsite.
            let occupied_names = PoolMember::process_names_set(&members);
            let mut spawned = 0u32;
            for slot in 0..u32::MAX {
                if spawned >= count {
                    break;
                }
                let proc_name = member_process_name(&name, &pool_uid, slot);
                if occupied_names.contains(&proc_name) {
                    continue;
                }
                let proc = build_member_process(&pool, &proc_name, slot, &name)?;
                // Create-verb dispatch rides the substrate primitive
                // `tatara_process::create::default` — pre-lift this was
                // a hand-authored `process_api.create(&PostParams::
                // default(), &proc)` chain, one of FIVE workspace-wide
                // restatements past the ★★ PRIME-DIRECTIVE ≥ 2
                // duplication threshold (peer at the desired-loop
                // spawn branch below + the reconciler ProcessTable
                // seed + the watcher allocation create + the probe
                // receipt-ConfigMap seed). Post-lift the create-verb
                // family lives at ONE substrate owner (sibling to
                // `patch::merge` on the wire-verb axis + `patch::
                // apply_patch_params` on the wire-posture axis).
                match tatara_process::create::default(&process_api, &proc).await {
                    Ok(_) => {
                        info!(namespace = %ns, pool = %name, process = %proc_name, "spawned member");
                        spawned += 1;
                    }
                    // 409 detection rides the substrate primitive
                    // `tatara_process::kube_error::is_conflict` — pre-
                    // lift this was a hand-authored
                    // `Err(kube::Error::Api(e)) if e.code == 409`
                    // match-arm guard, one of FIVE workspace-wide
                    // restatements past the ★★ PRIME-DIRECTIVE ≥ 2
                    // duplication threshold (peer at the desired-loop
                    // spawn branch below + the watcher + probe sites).
                    Err(ref e) if tatara_process::kube_error::is_conflict(e) => {
                        // race — someone else created this Process; treat as ok.
                        spawned += 1;
                    }
                    Err(e) => {
                        warn!(error = %e, "spawn failed; will retry");
                        break;
                    }
                }
            }
        }
        PoolDecision::ReapExcess { count } => {
            // Reap Free members first (never Allocated).
            let to_reap: Vec<_> = members
                .iter()
                .filter(|m| matches!(m.state, MemberState::Free))
                .take(count as usize)
                .collect();
            for m in to_reap {
                // `Api::delete(name, &DeleteParams::default())` routes
                // through the ONE substrate primitive
                // `tatara_process::delete::default` — see the
                // pre-lift audit note on the peer `SignalSigterm`
                // arm below for the full seven-site duplication
                // provenance.
                let _ = tatara_process::delete::default(&process_api, &m.process_name).await;
                info!(namespace = %ns, pool = %name, process = %m.process_name, "reaped excess");
            }
        }
        PoolDecision::ReplaceMembers { process_names } => {
            for n in process_names {
                let _ = tatara_process::delete::default(&process_api, &n).await;
                info!(namespace = %ns, pool = %name, process = %n, "replaced (deleted; respawn next tick)");
            }
        }
        PoolDecision::Drain => {
            for m in &members {
                let _ = tatara_process::delete::default(&process_api, &m.process_name).await;
            }
        }
    }

    // 4. Update Pool status.
    //
    // Legacy allocation-driven (`desired == 0`) status-patch seed —
    // sibling of the desired-count status-patch seed above; both ride
    // the substrate constructor `PoolStatus::observed`. See the peer
    // site's pre-lift audit note for the duplication history.
    let phase = pool_phase_from_members(&pool, &members);
    let _ = tatara_process::patch::merge_status(
        &pool_api,
        &name,
        &PoolStatus::observed(phase, members.clone(), Utc::now()),
    )
    .await;

    Ok(tatara_process::requeue::after_secs(
        ctx.config.heartbeat_seconds,
    ))
}

pub fn error_policy(
    _pool: Arc<EphemeralPool>,
    err: &ReconcilerError,
    _ctx: Arc<PoolContext>,
) -> Action {
    warn!(error = ?err, "pool reconcile failed");
    tatara_process::requeue::after_secs(15)
}

fn pool_phase_from_members(pool: &EphemeralPool, members: &[PoolMember]) -> PoolPhase {
    if pool.is_being_deleted() {
        return PoolPhase::Draining;
    }
    // The (free + spawning) supply calc lives at one site: the
    // `MemberState::counts_toward_supply` closed-set predicate. A
    // future variant that should also count toward supply (e.g. a
    // "Warming" state between Spawning and Free) lands at one
    // predicate arm in `tatara-process::pool::MemberState`; this site
    // inherits the new bucketing automatically. The
    // `member_state_failed_implies_no_supply` disjointness contract
    // in tatara-process pins that a Failed member can never inflate
    // this count.
    let supply = members
        .iter()
        .filter(|m| m.state.counts_toward_supply())
        .count() as u32;
    let want = pool.spec.desired_size;
    if members.is_empty() {
        return PoolPhase::Initializing;
    }
    if pool.spec.min_size > 0 && supply < pool.spec.min_size {
        return PoolPhase::Degraded;
    }
    if supply < want {
        return PoolPhase::ScalingUp;
    }
    if supply > want {
        return PoolPhase::ScalingDown;
    }
    PoolPhase::Steady
}

/// **R11 desired-count action applier.** Translates a
/// `Vec<ConvergenceAction>` from `decide_pool_convergence` into
/// kube-rs create/delete calls. Pure separation: the decision is
/// upstream, this is the I/O glue.
async fn apply_convergence_actions(
    pool: &EphemeralPool,
    process_api: &Api<Process>,
    ns: &str,
    name: &str,
    members: &[PoolMember],
    actions: Vec<ConvergenceAction>,
) -> Result<()> {
    if actions.is_empty() {
        return Ok(());
    }

    // Slot-slug seed rides through the substrate primitive
    // `EphemeralPool::owned_uid_or_name_or_empty` — see the peer
    // callsite in `reconcile_inner`'s `PoolDecision::Spawn` arm above
    // for the full rationale (paired `(metadata.uid, metadata.name)`
    // projection past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    // threshold; ONE substrate owner for both spawn arms).
    let pool_uid = pool.owned_uid_or_name_or_empty();
    // Peer to the legacy allocation-driven Spawn arm above: the
    // occupied-names collision-set rides through the substrate
    // primitive
    // [`tatara_process::pool::PoolMember::process_names_set`]. See
    // the sibling callsite's pre-lift audit note in
    // `reconcile_inner`'s `PoolDecision::Spawn` arm for the full
    // duplication history.
    let occupied_names = PoolMember::process_names_set(members);

    // Track the next free slot index per call so multiple
    // CreateMember actions in one tick spawn distinct names.
    let mut next_slot: u32 = 0;

    for action in actions {
        match action {
            ConvergenceAction::CreateMember => {
                // Find the next available slot name not currently in
                // use by another member.
                let proc_name = loop {
                    let candidate = member_process_name(name, &pool_uid, next_slot);
                    next_slot += 1;
                    if !occupied_names.contains(&candidate) {
                        break candidate;
                    }
                    if next_slot > u32::MAX / 2 {
                        return Err(anyhow!("pool {name} exhausted slot space"));
                    }
                };
                let proc = build_member_process(pool, &proc_name, next_slot - 1, name)?;
                // Create-verb dispatch rides the substrate primitive
                // `tatara_process::create::default` — desired-loop
                // spawn peer of the spawn-branch arm above; both share
                // ONE substrate owner post-lift.
                match tatara_process::create::default(&process_api, &proc).await {
                    Ok(_) => {
                        info!(namespace = %ns, pool = %name, process = %proc_name, "desired-loop spawned");
                    }
                    // 409 detection rides the substrate primitive
                    // `tatara_process::kube_error::is_conflict` — the
                    // desired-loop spawn's peer of the spawn-branch
                    // arm above; both interpret the race as a no-op
                    // (the next reconcile picks up the existing
                    // Process).
                    Err(ref e) if tatara_process::kube_error::is_conflict(e) => {
                        // already exists — race with another reconcile; OK.
                    }
                    Err(e) => {
                        warn!(error = %e, pool = %name, "spawn failed; will retry next tick");
                        return Ok(());
                    }
                }
            }
            ConvergenceAction::SignalSigterm { process_name } => {
                // Delete the Process — the reconciler's finalizer +
                // SIGTERM cascade unfolds via the standard exit path.
                //
                // `Api::delete(name, &DeleteParams::default())` routes
                // through the ONE substrate primitive
                // `tatara_process::delete::default` — pre-lift this
                // was one of SEVEN workspace-wide hand-authored
                // restatements of the 2-link chain past the ★★ PRIME-
                // DIRECTIVE ≥ 2 duplication threshold (five in this
                // controller_pool alone across pool-decision +
                // convergence-action arms, one in the reconciler
                // SIGTERM-cascade fan-out, one in the watcher
                // PR-close arm). Post-lift every DELETE-verb consumer
                // shares ONE substrate owner alongside the peer
                // create / patch primitives already lifted in
                // `tatara_process::{create,patch}`.
                let _ = tatara_process::delete::default(&process_api, &process_name).await;
                info!(
                    namespace = %ns,
                    pool = %name,
                    process = %process_name,
                    "desired-loop scale-down (SIGTERM oldest excess)"
                );
            }
            ConvergenceAction::ReapFailed { process_name } => {
                let _ = tatara_process::delete::default(&process_api, &process_name).await;
                info!(
                    namespace = %ns,
                    pool = %name,
                    process = %process_name,
                    "desired-loop reaped failed member"
                );
            }
            ConvergenceAction::Pause { reason } => {
                // Pool-wide pause is operator-visible via the
                // PoolStatus message; we don't apply further actions
                // this tick. Operator unpauses by acknowledging the
                // failed member (HoldFailed + manual reap) OR by
                // changing replacement_policy.
                warn!(
                    pool = %name,
                    reason = %reason,
                    "desired-loop paused — operator action required"
                );
                return Ok(());
            }
        }
    }
    Ok(())
}

fn process_belongs_to_pool(p: &Process, pool_name: &str) -> bool {
    // Annotation lookup via the substrate primitive — pre-lift this
    // was a hand-authored 3-line `.metadata.annotations.as_ref()
    // .and_then(|a| a.get(ANNOTATION_POOL)).map(String::as_str)`
    // chain, one of THREE workspace-wide restatements past the ★★
    // PRIME-DIRECTIVE ≥ 2 duplication threshold (peers at
    // `tatara-reconciler::signals::ingest` and
    // `tatara-reconciler::phase_machine::released_from_annotation`).
    // Post-lift the three lookups route through ONE substrate owner
    // [`tatara_process::prelude::Process::annotation`]; this callsite
    // composes its `== Some(pool_name)` equality tail at its own site,
    // preserving the pre-lift membership-gate semantics byte-for-byte.
    // The annotation KEY rides through the substrate constant
    // [`tatara_process::annotations::POOL`] — sibling to the paired
    // [`tatara_process::annotations::POOL_SLOT`] the write site below
    // (`build_member_process`) stamps in the same map. Pre-lift the
    // two keys were bare `"tatara.pleme.io/…"` string literals at the
    // file-scope `ANNOTATION_POOL` / `ANNOTATION_SLOT` consts here
    // AND at four reader-side test sites in
    // `tatara-process/src/lib.rs` + `crd.rs`; post-lift the writer
    // and every reader route through ONE substrate owner so any
    // future rename (a `tatara.pleme.io/v2/pool` migration, an alias
    // table for cross-cluster pool identity) lands at ONE `pub const`
    // in `tatara_process::annotations`.
    p.annotation(annotations::POOL) == Some(pool_name)
}

fn process_to_member_state(p: &Process) -> MemberState {
    use tatara_process::phase::ProcessPhase;
    // Route the phase-observed lookup through the substrate primitive
    // [`tatara_process::prelude::Process::observed_phase`] — pre-lift the
    // 3-line `.status.as_ref().map(|s| s.phase)` chain was hand-authored
    // here, duplicating the lift already living on `Process`. A future
    // normalization (generation filter, stale-observation gate) lands at
    // ONE substrate site and this member-state mapper inherits it
    // mechanically alongside every peer consumer in `tatara-reconciler`.
    match p.observed_phase() {
        Some(ProcessPhase::Attested) => {
            // Bound to an allocation iff lifetime is Ephemeral.
            if p.spec.lifetime.is_ephemeral() {
                MemberState::Allocated
            } else {
                MemberState::Free
            }
        }
        Some(ProcessPhase::Failed) | Some(ProcessPhase::Reaped) => MemberState::Failed,
        Some(ProcessPhase::Exiting | ProcessPhase::Zombie) => MemberState::Returning,
        _ => MemberState::Spawning,
    }
}

fn build_member_process(
    pool: &EphemeralPool,
    process_name: &str,
    slot: u32,
    pool_name: &str,
) -> Result<Process> {
    // Pool members start with Permanent lifetime — allocation flips
    // them to Ephemeral with the requestor's TTL.
    let template: EphemeralSpec = pool.spec.template.clone();
    let mut spec: ProcessSpec = template.into();
    spec.lifetime = Lifetime {
        permanent: Some(PermanentLifetime {}),
        ephemeral: None,
    };

    let mut proc = Process::new(process_name, spec);
    let ns = pool.metadata.namespace.clone();
    proc.metadata.namespace = ns;
    // Pool-membership annotation keys ride through the substrate
    // constants `tatara_process::annotations::{POOL, POOL_SLOT}` —
    // sibling to the [`tatara_process::annotations::{REQUESTOR,
    // ALLOCATION, REQUESTOR_KIND}`] allocator-bind axis-family
    // (`controller_allocation::reconcile_inner` Bind arm). Pre-lift
    // the two keys were file-scope `const ANNOTATION_POOL /
    // ANNOTATION_SLOT` in this module PLUS bare
    // `"tatara.pleme.io/pool"` string literals at four
    // reader-side test sites (in `tatara-process/src/lib.rs`'s
    // `annotated_tests` + in `tatara-process/src/crd.rs`'s
    // `annotation_composes_borrow_equality_tail_matching_pre_lift_pool`).
    // Post-lift the writer routes through ONE substrate owner per
    // key; the local binding stays shadowed as `metadata_annotations`
    // so the outer `annotations` module name in scope keeps
    // resolving to the substrate.
    let mut metadata_annotations = std::collections::BTreeMap::new();
    metadata_annotations.insert(annotations::POOL.to_string(), pool_name.to_string());
    metadata_annotations.insert(annotations::POOL_SLOT.to_string(), slot.to_string());
    proc.metadata.annotations = Some(metadata_annotations);

    // Owner reference so K8s cascade-deletes members on Pool deletion.
    if let (Some(uid), Some(name)) = (pool.metadata.uid.as_ref(), pool.metadata.name.as_ref()) {
        proc.metadata.owner_references = Some(vec![
            k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference {
                api_version: "tatara.pleme.io/v1alpha1".into(),
                kind: "EphemeralPool".into(),
                name: name.clone(),
                uid: uid.clone(),
                controller: Some(true),
                block_owner_deletion: Some(true),
            },
        ]);
    }

    let _ = POOL_FINALIZER;
    Ok(proc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_process::phase::ProcessPhase;

    #[test]
    fn process_to_member_state_attested_permanent_is_free() {
        // Routes through the ONE substrate composer
        // `ProcessSpec::gate_compute_defaults` — one of EIGHT pre-lift
        // exact-match sites past the ★★ PRIME-DIRECTIVE ≥ 2 threshold.
        let mut p = Process::new("x", ProcessSpec::gate_compute_defaults());
        p.status = Some(tatara_process::crd::ProcessStatus {
            phase: ProcessPhase::Attested,
            ..Default::default()
        });
        assert_eq!(process_to_member_state(&p), MemberState::Free);
    }

    #[test]
    fn process_to_member_state_attested_ephemeral_is_allocated() {
        let mut spec = ProcessSpec::gate_compute_defaults();
        spec.lifetime = Lifetime {
            ephemeral: Some(tatara_process::lifetime::EphemeralLifetime {
                ttl: "1h".into(),
                teardown_policy: tatara_process::lifetime::TeardownPolicy::Always,
                max_concurrent: 0,
                exports: vec![],
            }),
            ..Default::default()
        };
        let mut p = Process::new("y", spec);
        p.status = Some(tatara_process::crd::ProcessStatus {
            phase: ProcessPhase::Attested,
            ..Default::default()
        });
        assert_eq!(process_to_member_state(&p), MemberState::Allocated);
    }

    fn empty_spec() -> ProcessSpec {
        // Routes through the ONE substrate composer
        // `ProcessSpec::gate_compute_defaults` — sibling to the
        // `empty_spec` fixture in `tatara-process::crd::tests`.
        ProcessSpec::gate_compute_defaults()
    }

    #[test]
    fn belongs_to_pool_via_annotation() {
        let mut p = Process::new("x", empty_spec());
        let mut anns = std::collections::BTreeMap::new();
        anns.insert(annotations::POOL.into(), "demo-pool".into());
        p.metadata.annotations = Some(anns);
        assert!(process_belongs_to_pool(&p, "demo-pool"));
        assert!(!process_belongs_to_pool(&p, "other"));
    }
}
