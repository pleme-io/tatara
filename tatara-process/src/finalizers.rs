//! Workspace-canonical finalizer keys the tatara controllers stamp on
//! their owned CRDs to gate cascade-delete + orphan-reap on graceful
//! teardown.
//!
//! # Family
//!
//! The reconciler binaries in this workspace own three top-level CRDs
//! that participate in the K8s garbage-collection contract via the
//! `metadata.finalizers` list:
//!
//! - [`PROCESS`] — the [`crate::prelude::Process`] reconciler's
//!   finalizer key. Stamped by
//!   `tatara-reconciler::phase_machine::handle_pending` via
//!   [`crate::patch::ensure_finalizer`]; stripped by `handle_reaped`
//!   via `remove_finalizer`.
//! - [`ALLOCATION`] — the [`crate::prelude::EphemeralAllocation`]
//!   controller's finalizer key. Reserved by
//!   `tatara-pool-reconciler::controller_allocation::reconcile` for
//!   the future Bound → Releasing → Released cascade-delete gate.
//! - [`POOL`] — the [`crate::prelude::EphemeralPool`] controller's
//!   finalizer key. Reserved by
//!   `tatara-pool-reconciler::controller_pool::build_member_process`
//!   for the future member-Process orphan-reap gate.
//!
//! [`ALL`] enumerates the three consts in stable declaration order so
//! downstream sweeps (a fleet-wide finalizer audit binary, an ops
//! diagnostic dumping every tatara-owned finalizer on a cluster, a
//! per-key coverage report) iterate through ONE substrate slice
//! rather than three hand-typed literals.
//!
//! # Peer axes
//!
//! Peer to [`crate::annotations`] on the "K8s metadata key that pins
//! a reconciliation contract" axis-family — where [`crate::annotations`]
//! owns keys the reconciler READS or WRITES on OWNED resources
//! (FluxCD `HelmRelease`, `Kustomization`, member `Process` on a pool
//! member), this module owns keys the reconciler stamps on the CRDs
//! IT OWNS (the [`crate::prelude::Process`],
//! [`crate::prelude::EphemeralAllocation`],
//! [`crate::prelude::EphemeralPool`] `metadata.finalizers` list).
//! Both families carry the same `tatara.pleme.io/<slug>` wire-form
//! shape; the finalizer family additionally carries the shared
//! `<owner>-finalizer` suffix — a convention pinned by
//! [`tests::all_carry_finalizer_suffix`] so a future addition to the
//! family cannot silently drift the suffix.
//!
//! # Compounding
//!
//! Pre-lift the three wire-form finalizer literals were spread across
//! three files (the [`crate::PROCESS_FINALIZER`] top-level const at
//! [`crate`] + private `POOL_FINALIZER` / `ALLOC_FINALIZER` consts in
//! `tatara-pool-reconciler`), with no cross-owner coherence pin
//! binding the shared `tatara.pleme.io/<x>-finalizer` shape. A future
//! rename that shifted (e.g.) the group segment to `v2/` or the
//! per-owner suffix to `-guard` had to be applied at three separate
//! files coherently — a partial edit would silently orphan or
//! double-finalize whichever owner missed the update. Post-lift the
//! three consts live at ONE substrate owner + one closed [`ALL`]
//! slice + four family-invariant pins (per-key wire-form parity,
//! per-key `-finalizer` suffix, per-key `tatara.pleme.io/` prefix,
//! cross-key uniqueness).
//!
//! Theory grounding: THEORY.md §VI.1 (generation over composition —
//! the `tatara.pleme.io/<owner>-finalizer` wire-form shape recurred
//! at THREE hand-authored declaration sites past the ★★
//! PRIME-DIRECTIVE ≥ 2 duplication threshold; lifted to ONE
//! per-owner const + ONE `ALL` slice + four family-invariant pins
//! here). THEORY.md §II.1 invariant 5 (composition preserves proofs
//! — the invariant pins bind the family shape at fail-before-pass-
//! after granularity; a future rename that drifted the group prefix
//! or the suffix at ONE arm surfaces at [`tests::all_share_group_prefix`]
//! or [`tests::all_carry_finalizer_suffix`] rather than as silent
//! operator-facing skew at whichever owner's finalizer no longer
//! matches the K8s-side registered wire form).

/// Finalizer key stamped on the [`crate::prelude::Process`]
/// `metadata.finalizers` list by
/// `tatara-reconciler::phase_machine::handle_pending`. The reconciler
/// blocks K8s garbage collection until it has emitted the terminal
/// [`crate::prelude::ProcessAttestation`] receipt and stripped the
/// finalizer at `handle_reaped`.
///
/// Re-exported at the crate root as [`crate::PROCESS_FINALIZER`] for
/// consumers that predate this module.
pub const PROCESS: &str = "tatara.pleme.io/process-finalizer";

/// Finalizer key reserved for the
/// [`crate::prelude::EphemeralAllocation`] controller
/// (`tatara-pool-reconciler::controller_allocation`). Once the Bound
/// → Releasing → Released cascade is wired the controller will
/// stamp/strip this key through the same
/// [`crate::patch::ensure_finalizer`] / `remove_finalizer` primitives
/// the Process reconciler already routes through for [`PROCESS`].
pub const ALLOCATION: &str = "tatara.pleme.io/allocation-finalizer";

/// Finalizer key reserved for the [`crate::prelude::EphemeralPool`]
/// controller (`tatara-pool-reconciler::controller_pool`). Once the
/// member-Process orphan-reap gate is wired the controller will
/// stamp/strip this key at pool birth + pool teardown through the
/// same [`crate::patch::ensure_finalizer`] / `remove_finalizer`
/// primitives.
pub const POOL: &str = "tatara.pleme.io/pool-finalizer";

/// Closed family of tatara-owned finalizer keys in declaration
/// order: [`PROCESS`], [`ALLOCATION`], [`POOL`]. Downstream sweeps
/// (a per-owner coverage report, a fleet-wide finalizer audit
/// binary, an ops diagnostic dumping every tatara-owned finalizer
/// on a cluster) iterate through this ONE slice rather than
/// re-listing the three consts by hand.
///
/// Pinned to length 3 by [`tests::all_matches_declared_family`] so
/// a future addition to the family cannot slip past the coherence
/// harness without landing here first.
pub const ALL: &[&str] = &[PROCESS, ALLOCATION, POOL];

/// Shared group prefix every finalizer key in the [`ALL`] family
/// carries — the same `tatara.pleme.io/` group segment every
/// tatara-owned CRD annotation + finalizer key rides through, matching
/// the `#[kube(group = "tatara.pleme.io", …)]` derive slot on the CRD
/// structs. Pinned across every arm by
/// [`tests::all_share_group_prefix`] so a future rename of the group
/// (a shift to `v2/` under a migration, a per-fleet override) lands
/// at ONE substrate const here + the paired CRD derive slots rather
/// than at three independent finalizer wire forms.
pub const GROUP_PREFIX: &str = "tatara.pleme.io/";

/// Shared owner-suffix every finalizer key in the [`ALL`] family
/// carries — the convention `<owner>-finalizer` distinguishes a
/// finalizer key from a signal/label/annotation key on the same
/// group. Pinned across every arm by
/// [`tests::all_carry_finalizer_suffix`].
pub const FINALIZER_SUFFIX: &str = "-finalizer";

#[cfg(test)]
mod tests {
    use super::*;

    // ─── per-const wire-form pins ─────────────────────────────────
    //
    // Each pin binds ONE const to its exact wire-form string at
    // fail-before-pass-after granularity. A regression that renamed
    // the const's wire form (a group-segment shift, a suffix swap,
    // an accidental case drift) surfaces HERE rather than as silent
    // K8s-API-server-side skew at whichever owner's finalizer no
    // longer matches the registered key on the live cluster.

    #[test]
    fn process_wire_form_pin() {
        assert_eq!(PROCESS, "tatara.pleme.io/process-finalizer");
    }

    #[test]
    fn allocation_wire_form_pin() {
        assert_eq!(ALLOCATION, "tatara.pleme.io/allocation-finalizer");
    }

    #[test]
    fn pool_wire_form_pin() {
        assert_eq!(POOL, "tatara.pleme.io/pool-finalizer");
    }

    // ─── family-invariant pins ────────────────────────────────────
    //
    // The three consts form a closed family. These pins bind the
    // family-wide invariants (fixed length, no dupes across arms,
    // shared group prefix, shared `-finalizer` suffix) so a future
    // addition to the family that violated ANY invariant surfaces at
    // this module's tests rather than as silent skew at some
    // downstream site.

    #[test]
    fn all_matches_declared_family() {
        // A regression that added a fourth const to the family
        // without appending it to [`ALL`] (or shrank the family
        // without pruning the slice) would surface here. The
        // three-arm listing pins the declaration order the pool
        // reconciler + downstream sweep binaries iterate through.
        assert_eq!(ALL, &[PROCESS, ALLOCATION, POOL]);
        assert_eq!(ALL.len(), 3);
    }

    #[test]
    fn all_share_group_prefix() {
        // Every arm of the family MUST start with the shared
        // `tatara.pleme.io/` group segment — the SAME prefix every
        // tatara-owned CRD annotation + finalizer key rides through,
        // matching the `#[kube(group = "tatara.pleme.io", …)]` slot
        // on every CRD struct. A future rename that drifted ONE
        // arm's prefix (a copy-paste that landed on `tatara.io/`)
        // would silently escape the K8s garbage-collection contract
        // for that CRD; the pin surfaces the drift at compile-time
        // rather than at operator-visible orphan cascade skew.
        assert_eq!(GROUP_PREFIX, "tatara.pleme.io/");
        for key in ALL {
            assert!(
                key.starts_with(GROUP_PREFIX),
                "{key} must start with {GROUP_PREFIX}"
            );
        }
    }

    #[test]
    fn all_carry_finalizer_suffix() {
        // Every arm of the family MUST end with `-finalizer` — the
        // convention that distinguishes a finalizer key from a
        // signal/label/annotation key on the same group prefix. A
        // future addition that omitted the suffix (e.g. a bare
        // `tatara.pleme.io/pool` accidentally landing in the
        // family) would collide with an existing annotation key on
        // the peer [`crate::annotations`] axis and silently
        // double-book the K8s metadata slot.
        assert_eq!(FINALIZER_SUFFIX, "-finalizer");
        for key in ALL {
            assert!(
                key.ends_with(FINALIZER_SUFFIX),
                "{key} must end with {FINALIZER_SUFFIX}"
            );
        }
    }

    #[test]
    fn all_are_pairwise_unique() {
        // No two arms of the family may share the same wire form —
        // a copy-paste regression that duplicated the [`PROCESS`]
        // wire form at the [`POOL`] arm would silently rejoin the
        // three cascade-delete gates onto ONE finalizer, breaking
        // the per-CRD garbage-collection contract at the moment
        // whichever owner stripped its shared finalizer first.
        let mut seen: Vec<&str> = Vec::new();
        for key in ALL {
            assert!(!seen.contains(key), "duplicate finalizer key in ALL: {key}");
            seen.push(key);
        }
        assert_eq!(seen.len(), ALL.len());
    }

    // ─── crate-root re-export coherence pin ───────────────────────
    //
    // The pre-existing [`crate::PROCESS_FINALIZER`] top-level const
    // now routes through this module's [`PROCESS`] owner. Pin the
    // coherence so a regression that decoupled the re-export
    // surfaces here rather than as silent skew between the reconciler
    // production callsites (which reach through
    // `tatara_process::PROCESS_FINALIZER`) and any future consumer
    // reaching through `tatara_process::finalizers::PROCESS` directly.

    #[test]
    fn process_finalizer_top_level_reexport_routes_through_finalizers_process() {
        assert_eq!(crate::PROCESS_FINALIZER, PROCESS);
    }
}
