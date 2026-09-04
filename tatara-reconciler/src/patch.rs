//! Kube-client patch helpers.
//!
//! Status patches use `patch_status` (operates on the `/status` subresource).
//! Spec patches use `patch` (may trigger further reconciliation).
//!
//! All patches are `Patch::Merge` — simplest semantics; concurrent writes
//! are resolved by resourceVersion conflict retries at the controller level.

use chrono::Utc;
use kube::api::Api;
use kube::Error as KubeError;
use serde::Serialize;
use serde_json::{json, Value};

use tatara_process::phase::ProcessPhase;
use tatara_process::prelude::{Identity, Process, ProcessTable};
use tatara_process::table::ProcessTableSpec;

/// Merge-patch the status subresource of a Process.
///
/// Delegates the wire-body wrap + `PatchParams::default() +
/// Patch::Merge` chain to the substrate primitive
/// [`tatara_process::patch::merge_status`], the ONE owner of the
/// merge-status idiom across every workspace controller (peer of
/// `tatara_reconciler::ssapply::apply_patch_params` on the SSA axis).
/// The public shape (`Value` status body → `Result<Process,
/// KubeError>`) is preserved so existing callers (the
/// `SignalEffect::TransitionTo` / `ForceAttest` arms + every
/// `phase_machine.rs` transition writer) reach the substrate through
/// this thin, `Process`-typed wrapper without a per-consumer signature
/// churn.
pub async fn patch_process_status(
    api: &Api<Process>,
    name: &str,
    status_patch: Value,
) -> Result<Process, KubeError> {
    tatara_process::patch::merge_status(api, name, &status_patch).await
}

/// Merge-patch the spec of a ProcessTable (we keep `nextSequence` in spec
/// for parity with convergence-controller; future refactor may move it to
/// status).
///
/// Wire-side dispatch rides the substrate primitive
/// [`tatara_process::patch::merge`] — pre-lift this was a hand-authored
/// 3-link `api.patch(name, &PatchParams::default(), &Patch::Merge(&body))`
/// chain, one of SIX workspace-wide restatements past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication trigger. Post-lift the primary-
/// resource merge posture lives at ONE substrate owner (sibling to
/// [`tatara_process::patch::merge_status`] on the wire-endpoint axis
/// and to [`tatara_process::patch::apply_patch_params`] on the wire-
/// posture axis).
pub async fn patch_process_table_spec(
    api: &Api<ProcessTable>,
    name: &str,
    spec_patch: Value,
) -> Result<ProcessTable, KubeError> {
    let body = json!({ "spec": spec_patch });
    tatara_process::patch::merge(api, name, &body).await
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
    // Create-verb dispatch rides the substrate primitive
    // `tatara_process::create::default` — pre-lift this was a hand-
    // authored `api.create(&PostParams::default(), &pt)` chain, one of
    // FIVE workspace-wide restatements past the ★★ PRIME-DIRECTIVE ≥ 2
    // duplication threshold. Post-lift the create-verb family lives at
    // ONE substrate owner (sibling to `merge` on the wire-verb axis).
    tatara_process::create::default(api, &pt).await
}

/// The two-slot `{"phase": <phase>, "phaseSince": <now>}` byte-shape
/// every phase-transition status-patch builder in this module needs.
///
/// The three sibling primitives ([`phase_status`], [`phase_status_msg`],
/// [`phase_status_with`]) each attach a distinct third slot (`identity`
/// / `message` / a caller-named extra), but every one starts from the
/// SAME two-slot base. Pre-lift the base shape was open-coded verbatim
/// at all THREE sibling entries past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold — each restating `json!({ "phase": phase,
/// "phaseSince": Utc::now() })` byte-for-byte. Post-lift the base lives
/// at ONE owner and the three siblings delegate through it, so a future
/// promotion of the base shape (a `by:` field naming the signal source,
/// a `transitionCount:` diagnostic counter, a shared `at:` alias for
/// `phaseSince`, a promotion of `Utc::now()` to an injectable
/// `time_source` for deterministic tests) lands at ONE substrate
/// function and all THREE sibling patch-builders inherit the upgrade
/// mechanically.
///
/// The `Utc::now()` call is load-bearing at THIS site — every sibling
/// stamps the transition at CALL time, not at the transition-decision
/// time upstream, so a caller who cannot afford a wall-clock read at
/// the emit boundary must add a distinct sibling with an explicit
/// timestamp parameter rather than route through this base.
///
/// The primitive is `pub(super)` — the three sibling patch-builders in
/// this module are its only intended consumers; a downstream caller
/// that needs the base two-slot shape composes it through one of the
/// three sibling entries rather than reaching directly for the base
/// (which would bypass the third-slot discipline the siblings enforce
/// by construction).
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// two-slot phase+phaseSince base shape recurred at all THREE sibling
/// patch-builders past the PRIME-DIRECTIVE ≥ 2 trigger, and is lifted
/// to ONE substrate primitive here). THEORY.md §II.1 invariant 5
/// (composition preserves proofs — post-lift the "base shape at
/// phase_status == base shape at phase_status_msg == base shape at
/// phase_status_with" invariant holds by construction rather than by
/// three open-coded copies staying in sync; a regression that drifted
/// the base-slot naming or the timestamp posture at ONLY one sibling
/// surfaces at the primitive's tests rather than as operator-visible
/// wrong-phase / wrong-timestamp patches after apply).
pub(super) fn phase_status_base(phase: ProcessPhase) -> Value {
    json!({
        "phase": phase,
        "phaseSince": Utc::now(),
    })
}

/// Common status patch builder — phase + phaseSince, optionally identity.
///
/// Composes the shared two-slot base through [`phase_status_base`] and
/// attaches an optional `identity` third slot.
pub fn phase_status(phase: ProcessPhase, identity: Option<&Identity>) -> Value {
    let mut v = phase_status_base(phase);
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
    let mut v = phase_status_base(phase);
    v["message"] = Value::String(message.into());
    v
}

/// Status patch builder — phase + phaseSince + one extra sibling key/value
/// pair.
///
/// The `phase + phaseSince + <extra_key>: <extra_val>` shape recurred at
/// two hand-authored callsites in `phase_machine.rs` (Running-entry
/// attaching `"fluxResources": refs`, Attested-entry attaching
/// `"attestation": next`) at the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// threshold before this primitive existed; each site inlined `json!({
/// "phase": ..., "phaseSince": Utc::now(), <key>: <val> })` verbatim.
/// Post-lift both callsites collapse to one line each and a future
/// addition to the base shape (a `by:` field for the signal source, a
/// `transitionCount:` diagnostic counter, a structured message
/// envelope, a shared `at:` alias for `phaseSince`) lands at ONE
/// substrate function and both callsites inherit the upgrade
/// mechanically.
///
/// The extra key is `&'static str` — accepts only compile-time
/// literals, which is how both current callsites use it
/// (`"fluxResources"`, `"attestation"`). A `debug_assert!` catches a
/// regression where the extra key collides with one of the base slots
/// (`"phase"` / `"phaseSince"`), which would silently overwrite the
/// base slot on merge and corrupt the status patch shape.
///
/// The extra value is `impl Serialize` — accepts owned or borrowed
/// values of any serde-serialisable type without widening the
/// signature (both current callsites pass borrowed references:
/// `&Vec<FluxResourceRef>` and `&ProcessAttestation`). A serialisation
/// failure resolves to `Value::Null`, matching the existing
/// `phase_status(phase, identity)` primitive's posture.
pub fn phase_status_with<T: Serialize>(phase: ProcessPhase, key: &'static str, value: T) -> Value {
    debug_assert!(
        key != "phase" && key != "phaseSince",
        "phase_status_with: extra key `{key}` collides with a base slot",
    );
    let mut v = phase_status_base(phase);
    v[key] = serde_json::to_value(value).unwrap_or(Value::Null);
    v
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

/// Build the `metadata.finalizers` merge-patch body around a computed list.
///
/// Owns the two-slot `{"metadata": {"finalizers": [...]}}` byte-shape every
/// finalizer-write callsite needs. Before this primitive existed, the shape
/// was open-coded at two adjacent async wrappers (`ensure_finalizer` +
/// `remove_finalizer`) past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger;
/// post-lift the shape lives at ONE owner and both wrappers ride through
/// [`apply_finalizer_transform`]. A future field addition (a sibling
/// `metadata.annotations` slot naming the write reason, a preconditioned
/// patch keyed on the caller-observed resourceVersion, a structured
/// finalizer-envelope replacing the flat string list) lands here.
pub(super) fn finalizers_metadata_patch(new: &[String]) -> Value {
    json!({ "metadata": { "finalizers": new } })
}

/// Shared async delegator — reads existing finalizers, runs the pure
/// `transform`, and (if the transform produced a new list) merge-patches it
/// through [`finalizers_metadata_patch`].
///
/// Owns the byte-shape both `ensure_finalizer` + `remove_finalizer` share:
/// (a) `p.metadata.finalizers.clone().unwrap_or_default()` on the read
/// side, (b) `Ok(false)` early-return when the transform is a no-op, (c)
/// build the metadata patch through the shared primitive, (d) `api.patch`
/// with `PatchParams::default()` + `Patch::Merge`, (e) `Ok(true)` on
/// success. The two axis-typed public wrappers stay `pub` — their names
/// encode intent at the callsite (`ensure` vs `remove`) — but their bodies
/// are one line each: delegate through here with the axis-appropriate pure
/// computer (`add_finalizer` / `remove_finalizer_from`).
async fn apply_finalizer_transform<F>(
    api: &Api<Process>,
    name: &str,
    p: &Process,
    target: &str,
    transform: F,
) -> Result<bool, KubeError>
where
    F: FnOnce(&[String], &str) -> Option<Vec<String>>,
{
    let existing = p.metadata.finalizers.clone().unwrap_or_default();
    let Some(new) = transform(&existing, target) else {
        return Ok(false);
    };
    let patch = finalizers_metadata_patch(&new);
    // Wire-side dispatch rides the substrate primitive
    // `tatara_process::patch::merge` — pre-lift this was a hand-
    // authored `api.patch(name, &PatchParams::default(),
    // &Patch::Merge(&patch))` chain, one of SIX workspace-wide
    // restatements past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    // threshold. Post-lift the primary-resource merge posture lives at
    // ONE substrate owner (peer of `merge_status` on the `/status`
    // subresource axis + `apply_patch_params` on the SSA wire-posture
    // axis).
    tatara_process::patch::merge(api, name, &patch).await?;
    Ok(true)
}

/// Add the tatara finalizer to a Process if not already present.
///
/// Delegates through the shared [`apply_finalizer_transform`] owner.
pub async fn ensure_finalizer(
    api: &Api<Process>,
    name: &str,
    p: &Process,
    target: &str,
) -> Result<bool, KubeError> {
    apply_finalizer_transform(api, name, p, target, add_finalizer).await
}

/// Remove the tatara finalizer from a Process if present — allows K8s GC to proceed.
///
/// Delegates through the shared [`apply_finalizer_transform`] owner.
pub async fn remove_finalizer(
    api: &Api<Process>,
    name: &str,
    p: &Process,
    target: &str,
) -> Result<bool, KubeError> {
    apply_finalizer_transform(api, name, p, target, remove_finalizer_from).await
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

    // ─── phase_status_base substrate pins ─────────────────────────────────
    //
    // The two-slot `{"phase": ..., "phaseSince": Utc::now()}` base shape
    // was open-coded verbatim at all THREE sibling patch-builders
    // (`phase_status`, `phase_status_msg`, `phase_status_with`) before
    // `phase_status_base` closed it. These pins bind the base at
    // fail-before-pass-after granularity so a regression that drifted a
    // slot key, reshaped the timestamp posture, dropped the base's
    // Utc::now() stamping to a stale constant, or leaked a sibling slot
    // into the base surfaces HERE rather than as operator-visible drift
    // across all three sibling patch-builders simultaneously.

    #[test]
    fn phase_status_base_composes_the_two_slots() {
        // The primary shape asserted end-to-end: exactly ONE `phase`
        // slot + ONE `phaseSince` slot, no third-slot leaks. A
        // regression that pulled ONE sibling's third slot (`identity` /
        // `message` / a caller-named extra) into the base would double-
        // stamp that slot at every sibling and surface here.
        let v = phase_status_base(ProcessPhase::Running);
        let obj = v.as_object().expect("object");
        assert_eq!(
            obj.len(),
            2,
            "base owns exactly two slots — no third-slot leaks"
        );
        assert_eq!(
            obj.get("phase").and_then(Value::as_str),
            Some("Running"),
            "phase key present and serialises as the discriminant string",
        );
        assert!(
            tatara_process::time::parse_rfc3339_opt(obj.get("phaseSince").and_then(Value::as_str))
                .is_some(),
            "phaseSince key present and is a valid RFC-3339 timestamp",
        );
        // The sibling third-slot keys must NOT leak into the base — a
        // regression that hoisted one of the sibling extras into the
        // base would double-stamp at every sibling caller.
        for leaked in ["identity", "message", "fluxResources", "attestation"] {
            assert!(
                obj.get(leaked).is_none(),
                "base must not carry sibling third-slot key `{leaked}` — that discipline lives at the sibling entry",
            );
        }
    }

    #[test]
    fn phase_status_base_stamps_phase_since_at_call_time() {
        // A regression that hoisted the `Utc::now()` to a stale
        // constant (e.g. cached at module load) or moved it upstream
        // to a caller argument would surface here at every wall-clock
        // bracket. Pins the base's own stamping discipline
        // independently of the three siblings (whose own tests already
        // pin their delegates).
        let before = Utc::now();
        let v = phase_status_base(ProcessPhase::Attested);
        let after = Utc::now();
        let stamped =
            tatara_process::time::parse_rfc3339_opt(v.get("phaseSince").and_then(Value::as_str))
                .expect("phaseSince is an RFC-3339 timestamp");
        let stamped_utc = stamped.with_timezone(&chrono::Utc);
        assert!(
            (before..=after).contains(&stamped_utc),
            "phaseSince ({stamped_utc}) must land in [{before}, {after}] — the base stamps at call time",
        );
    }

    #[test]
    fn phase_status_base_matches_hand_authored_pre_lift_bytewise() {
        // Byte-identical parity with the pre-lift `json!({ "phase":
        // <phase>, "phaseSince": Utc::now() })` block that ALL THREE
        // sibling patch-builders restated verbatim, swept across five
        // representative ProcessPhase discriminants so a regression
        // that broke ONE phase's serialisation (e.g. an enum re-tagging
        // that swapped the discriminant projection) surfaces per-phase.
        //
        // Both blocks build a fresh Utc::now() so the two `phaseSince`
        // stamps CAN differ by the wall-clock delta between calls;
        // drop that slot before comparing.
        for phase in [
            ProcessPhase::Pending,
            ProcessPhase::Execing,
            ProcessPhase::Running,
            ProcessPhase::Attested,
            ProcessPhase::Failed,
        ] {
            let mut composed = phase_status_base(phase);
            let mut hand_authored = json!({
                "phase": phase,
                "phaseSince": Utc::now(),
            });
            composed
                .as_object_mut()
                .expect("object")
                .remove("phaseSince");
            hand_authored
                .as_object_mut()
                .expect("object")
                .remove("phaseSince");
            assert_eq!(
                composed, hand_authored,
                "base must be byte-identical to the pre-lift json! block for phase `{phase}`",
            );
        }
    }

    #[test]
    fn phase_status_base_is_shared_owner_of_all_three_siblings() {
        // Cross-sibling parity — every sibling patch-builder
        // (`phase_status`, `phase_status_msg`, `phase_status_with`)
        // MUST carry the base's two slots verbatim (modulo the
        // wall-clock delta between the two `phaseSince` stamps). A
        // regression that stopped routing ONE sibling through the base
        // (open-coding it again at the sibling) would drift that
        // sibling's base-slot naming or timestamp posture — this pin
        // catches it by asserting all three siblings' base slots are
        // structurally identical to the base primitive's own output.
        //
        // Sibling posture to the pre-lift-byte-identity peers on each
        // sibling above: those pin each sibling against a HAND-AUTHORED
        // pre-lift json! block; this pin closes the OTHER direction —
        // each sibling's emission carries the SAME base primitive's
        // slots, so drift at any sibling's delegation surfaces here.
        let base = phase_status_base(ProcessPhase::Running);
        let bare = phase_status(ProcessPhase::Running, None);
        let msg = phase_status_msg(ProcessPhase::Running, "reason");
        let extra = phase_status_with(ProcessPhase::Running, "extraKey", serde_json::json!(1));
        // All four carry the same phase discriminant under the same
        // `phase` slot.
        for (name, v) in [
            ("base", &base),
            ("bare", &bare),
            ("msg", &msg),
            ("extra", &extra),
        ] {
            assert_eq!(
                v.get("phase").and_then(Value::as_str),
                Some("Running"),
                "{name} must carry the shared `phase` slot with the same discriminant",
            );
            assert!(
                tatara_process::time::parse_rfc3339_opt(
                    v.get("phaseSince").and_then(Value::as_str)
                )
                .is_some(),
                "{name} must carry the shared `phaseSince` slot as an RFC-3339 timestamp",
            );
        }
        // Slot-count guard: the base has 2, bare has 2 (no identity),
        // msg has 3 (with message), extra has 3 (with the caller-named
        // key). A regression that leaked an extra slot into the base
        // would inflate every sibling's slot count in lockstep.
        assert_eq!(base.as_object().unwrap().len(), 2);
        assert_eq!(bare.as_object().unwrap().len(), 2);
        assert_eq!(msg.as_object().unwrap().len(), 3);
        assert_eq!(extra.as_object().unwrap().len(), 3);
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
            tatara_process::time::parse_rfc3339_opt(obj.get("phaseSince").and_then(Value::as_str))
                .is_some(),
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
        let stamped =
            tatara_process::time::parse_rfc3339_opt(v.get("phaseSince").and_then(Value::as_str))
                .expect("phaseSince is an RFC-3339 timestamp");
        let stamped_utc = stamped.with_timezone(&chrono::Utc);
        assert!(
            (before..=after).contains(&stamped_utc),
            "phaseSince ({stamped_utc}) must land in [{before}, {after}] — the primitive stamps at call time"
        );
    }

    // ─── finalizers_metadata_patch substrate pins ─────────────────────────
    //
    // The `json!({ "metadata": { "finalizers": <list> } })` shape was open-
    // coded at two adjacent async wrappers (`ensure_finalizer` +
    // `remove_finalizer`) at the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    // threshold before the primitive existed. Both wrappers now route
    // through `apply_finalizer_transform`, whose patch-body slot is
    // `finalizers_metadata_patch`. These pins bind the shape at
    // fail-before-pass-after granularity so a regression that renamed a
    // key, added a sibling slot, or reshaped the finalizer list surfaces
    // HERE rather than as silent operator-visible drift across both
    // wrappers simultaneously.

    #[test]
    fn finalizers_metadata_patch_wraps_list_in_two_slot_metadata_body() {
        let list = vec![
            "tatara.pleme.io/process-finalizer".to_string(),
            "other.io/finalizer".to_string(),
        ];
        let v = finalizers_metadata_patch(&list);
        let obj = v.as_object().expect("root is object");
        // ONE top-level slot: `metadata` — no `spec`/`status` sibling leaks.
        assert_eq!(obj.len(), 1);
        let metadata = obj.get("metadata").and_then(Value::as_object).expect(
            "root.metadata present as object; a regression that renamed the slot surfaces here",
        );
        // ONE metadata slot: `finalizers` — no sibling annotations/labels
        // leak that would silently overwrite unrelated metadata on merge.
        assert_eq!(metadata.len(), 1);
        let finalizers = metadata
            .get("finalizers")
            .and_then(Value::as_array)
            .expect("metadata.finalizers is an array (K8s expects list, never null/object)");
        assert_eq!(finalizers.len(), 2);
        assert_eq!(
            finalizers[0].as_str(),
            Some("tatara.pleme.io/process-finalizer")
        );
        assert_eq!(finalizers[1].as_str(), Some("other.io/finalizer"));
    }

    #[test]
    fn finalizers_metadata_patch_empty_list_emits_empty_array_not_null() {
        // The remove-last-finalizer path lands here — K8s reads an empty
        // array as "no finalizers, GC may proceed"; a regression that
        // emitted `null` instead would silently keep the finalizer in
        // place (merge-patch treats `null` as delete-the-key rather than
        // set-empty-list) and the process would never reap.
        let v = finalizers_metadata_patch(&[]);
        let arr = v
            .pointer("/metadata/finalizers")
            .and_then(Value::as_array)
            .expect("empty-list path still emits an array");
        assert!(arr.is_empty());
        assert!(!v.pointer("/metadata/finalizers").is_none_or(Value::is_null));
    }

    #[test]
    fn finalizers_metadata_patch_matches_hand_authored_pre_lift_bytewise() {
        // Byte-identical parity with the pre-lift `json!({ "metadata": {
        // "finalizers": new } })` block that both `ensure_finalizer` +
        // `remove_finalizer` restated verbatim. Pinned across the three
        // representative paths: empty-list (remove-last), single-entry
        // (add-first / remove-with-siblings), multi-entry (add-with-
        // siblings / remove-middle). A regression that reshaped the
        // primitive's output would diverge from the pre-lift block HERE
        // rather than at every downstream K8s round-trip.
        for new in [
            Vec::<String>::new(),
            vec!["tatara.pleme.io/process-finalizer".to_string()],
            vec![
                "a".to_string(),
                "tatara.pleme.io/process-finalizer".to_string(),
                "b".to_string(),
            ],
        ] {
            let composed = finalizers_metadata_patch(&new);
            let hand_authored = json!({ "metadata": { "finalizers": new } });
            assert_eq!(
                composed, hand_authored,
                "primitive must be byte-identical to the pre-lift json! block for list {new:?}"
            );
        }
    }

    #[test]
    fn finalizers_metadata_patch_preserves_slice_order() {
        // K8s merges finalizers as an ordered list on the wire; a
        // regression that reordered (e.g. sorted for determinism) would
        // stomp caller-intended insertion order and could reorder deletion
        // dependency: this pins the slice-in, slice-out invariant.
        let list = vec!["z".to_string(), "a".to_string(), "m".to_string()];
        let arr = finalizers_metadata_patch(&list)
            .pointer("/metadata/finalizers")
            .cloned()
            .and_then(|v| v.as_array().cloned())
            .expect("array present");
        assert_eq!(
            arr.iter().filter_map(Value::as_str).collect::<Vec<_>>(),
            vec!["z", "a", "m"],
            "order must ride through the primitive verbatim"
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

    // ─── phase_status_with substrate pins ─────────────────────────────────
    //
    // The three-slot `phase + phaseSince + <extra_key>: <extra_val>` shape
    // recurred at two hand-authored callsites (Running-entry attaching
    // `fluxResources`, Attested-entry attaching `attestation`) at the ★★
    // PRIME-DIRECTIVE ≥ 2 duplication threshold before `phase_status_with`
    // closed it. These pins bind the primitive at fail-before-pass-after
    // granularity so a regression that dropped a slot, renamed a key,
    // reshaped the extra value, or silently overwrote a base slot surfaces
    // HERE rather than as operator-visible drift at every downstream
    // transition.

    #[test]
    fn phase_status_with_composes_the_three_slots() {
        // The primary shape asserted end-to-end: exactly ONE `phase` slot
        // + ONE `phaseSince` slot + ONE caller-named extra slot, no
        // sibling leaks (a `spec`/`status`/`message` leak that would
        // silently overwrite unrelated status keys on merge surfaces
        // here).
        let v = phase_status_with(
            ProcessPhase::Running,
            "fluxResources",
            serde_json::json!([]),
        );
        let obj = v.as_object().expect("object");
        assert_eq!(obj.len(), 3);
        assert_eq!(
            obj.get("phase").and_then(Value::as_str),
            Some("Running"),
            "phase key present and serialises as the discriminant string"
        );
        assert!(
            tatara_process::time::parse_rfc3339_opt(obj.get("phaseSince").and_then(Value::as_str))
                .is_some(),
            "phaseSince key present and is a valid RFC-3339 timestamp"
        );
        assert!(
            obj.get("fluxResources").and_then(Value::as_array).is_some(),
            "extra key present under its caller-named slot",
        );
    }

    #[test]
    fn phase_status_with_stamps_phase_since_at_call_time() {
        let before = Utc::now();
        let v = phase_status_with(ProcessPhase::Attested, "attestation", serde_json::json!({}));
        let after = Utc::now();
        let stamped =
            tatara_process::time::parse_rfc3339_opt(v.get("phaseSince").and_then(Value::as_str))
                .expect("phaseSince is an RFC-3339 timestamp");
        let stamped_utc = stamped.with_timezone(&chrono::Utc);
        assert!(
            (before..=after).contains(&stamped_utc),
            "phaseSince ({stamped_utc}) must land in [{before}, {after}] — the primitive stamps at call time"
        );
    }

    #[test]
    fn phase_status_with_matches_hand_authored_pre_lift_bytewise() {
        // Byte-identical parity with the pre-lift `json!({ "phase": ...,
        // "phaseSince": Utc::now(), <key>: <val> })` block that both
        // callsites restated verbatim, swept across the two representative
        // axes: (a) key=`"fluxResources"`, value=Vec<obj> (Running-entry
        // shape), (b) key=`"attestation"`, value=obj (Attested-entry
        // shape). A regression that reshaped the primitive would diverge
        // from the pre-lift block HERE rather than at every downstream
        // K8s round-trip.
        //
        // Both blocks build a fresh Utc::now() so the two `phaseSince`
        // stamps CAN differ by the wall-clock delta between calls; drop
        // that slot before comparing.
        for (key, value) in [
            (
                "fluxResources",
                serde_json::json!([
                    {"kind": "Kustomization", "name": "a", "namespace": "ns"},
                    {"kind": "HelmRelease",   "name": "b", "namespace": "ns"},
                ]),
            ),
            (
                "attestation",
                serde_json::json!({
                    "generation": 1,
                    "composed_root": "deadbeef",
                }),
            ),
        ] {
            let mut composed = phase_status_with(ProcessPhase::Running, key, value.clone());
            let mut hand_authored = json!({
                "phase": ProcessPhase::Running,
                "phaseSince": Utc::now(),
                key: value,
            });
            composed
                .as_object_mut()
                .expect("object")
                .remove("phaseSince");
            hand_authored
                .as_object_mut()
                .expect("object")
                .remove("phaseSince");
            assert_eq!(
                composed, hand_authored,
                "primitive must be byte-identical to the pre-lift json! block for key `{key}`"
            );
        }
    }

    #[test]
    fn phase_status_with_serialises_borrowed_typed_values() {
        // Both current callsites pass BORROWED references
        // (`&Vec<FluxResourceRef>`, `&ProcessAttestation`) — the primitive's
        // `T: Serialize` bound must accept them without a widening
        // signature. Pinned against a `&Vec<i32>` (structurally a Vec of
        // Serialize) and a `&BTreeMap<String, String>` (structurally an
        // object of Serialize) — both representative of the shapes the
        // production callsites feed.
        use std::collections::BTreeMap;

        let refs: Vec<i32> = vec![1, 2, 3];
        let vec_val = phase_status_with(ProcessPhase::Running, "list", &refs);
        assert_eq!(
            vec_val.get("list").and_then(Value::as_array).map(Vec::len),
            Some(3),
            "borrowed &Vec<Serialize> rides through as an array of length 3"
        );

        let mut map: BTreeMap<String, String> = BTreeMap::new();
        map.insert("k".to_string(), "v".to_string());
        let map_val = phase_status_with(ProcessPhase::Attested, "obj", &map);
        assert_eq!(
            map_val.pointer("/obj/k").and_then(Value::as_str),
            Some("v"),
            "borrowed &BTreeMap<String, String> rides through as a nested object"
        );
    }

    #[test]
    #[should_panic(expected = "collides with a base slot")]
    fn phase_status_with_debug_asserts_on_phase_slot_collision() {
        // A regression that passed `key = "phase"` would silently
        // overwrite the base `phase` slot with the extra value on
        // insert-order-loss and the operator would see the wrong phase on
        // the wire; the debug_assert catches it at the primitive.
        let _ = phase_status_with(
            ProcessPhase::Running,
            "phase",
            serde_json::json!("Attested"),
        );
    }

    #[test]
    #[should_panic(expected = "collides with a base slot")]
    fn phase_status_with_debug_asserts_on_phase_since_slot_collision() {
        // Peer pin on the `phaseSince` axis — a regression that passed
        // `key = "phaseSince"` would silently overwrite the base
        // timestamp with the extra value and the operator would see the
        // wrong phase-transition instant on the wire; the debug_assert
        // catches it at the primitive.
        let _ = phase_status_with(
            ProcessPhase::Attested,
            "phaseSince",
            serde_json::json!("1970-01-01T00:00:00Z"),
        );
    }
}
