//! Substrate predicates over [`kube::Error`] — the semantic layer
//! every controller's `match … { Err(kube::Error::Api(e)) if e.code
//! == <N> => … }` guard restates by hand pre-lift.
//!
//! Owns two closed-set predicates over the `kube::Error::Api`
//! sub-variant's HTTP status code:
//!
//! * [`is_conflict`] — HTTP 409 (Conflict) — the K8s API server
//!   refused a `create` because a resource with the same key already
//!   exists, or refused a `patch` because of an optimistic-concurrency
//!   generation mismatch. Every controller's create-branch reads this
//!   arm as "someone else got here first; treat the intended write as
//!   already-done" or "refresh via PATCH".
//! * [`is_not_found`] — HTTP 404 (Not Found) — the K8s API server has
//!   no resource with the given key. Every controller's delete-branch
//!   reads this arm as "already deleted / never existed; the intended
//!   state (absence) is already true".
//!
//! Both predicates lift the 2-link `matches!(err, kube::Error::Api(e)
//! if e.code == <N>)` shape past the ★★ PRIME-DIRECTIVE ≥ 2
//! duplication trigger. Pre-lift the SAME chain was hand-authored at
//! FIVE workspace-wide sites, each interpreting the same HTTP status
//! code with the same semantic:
//!
//! * `tatara-closed-loop-probe::write_receipt_configmap` — 409 arm on
//!   `api.create(...)` → falls through to a merge-patch of the `data`
//!   field so the receipt payload lands idempotently.
//! * `tatara-github-watcher::handler::handle_pr_event` — 404 arm on
//!   `api.delete(...)` → returns `200 OK "allocation already gone"`
//!   so a closed-PR event is a no-op past the first delivery.
//! * `tatara-github-watcher::handler::handle_pr_event` — 409 arm on
//!   `api.create(...)` → returns `200 OK "allocation already exists
//!   (synchronize)"` so a re-delivery of an `opened` PR event maps
//!   onto the existing allocation.
//! * `tatara-pool-reconciler::controller_pool` (spawn branch, spawn
//!   loop) — 409 arm on `process_api.create(...)` → treats the race
//!   as a successful spawn, incrementing `spawned` past the arm.
//! * `tatara-pool-reconciler::controller_pool` (desired-loop branch)
//!   — 409 arm on `process_api.create(...)` → treats the race as a
//!   no-op so the next reconcile picks up the existing Process.
//!
//! All FIVE sites walked the SAME two-link shape — destructure the
//! `kube::Error::Api` sub-variant, guard on `e.code == <N>` — and
//! interpret the code identically ("write already succeeded" / "delete
//! already succeeded"). The `e: ErrorResponse` binding is bound but
//! unused at every callsite; the body reads the SEMANTIC (conflict /
//! not-found) rather than the specific fields (`e.reason`, `e.message`).
//! Post-lift each callsite reads
//! `Err(ref e) if kube_error::is_conflict(e) => { ... }` (or
//! `is_not_found`), and the two-link shape lives at ONE substrate
//! owner.
//!
//! ### Semantic axis (why predicates, not raw codes)
//!
//! The K8s API server sends the same HTTP status code for a set of
//! semantically identical outcomes (a 404 on `get` and a 404 on
//! `delete` both mean "the resource is not present"); it also
//! occasionally sends the same code for OTHER outcomes with subtly
//! different meanings (a 404 on a subresource whose parent exists,
//! for instance). Lifting the raw-code check to a NAMED predicate
//! moves every consumer onto the semantic axis, so a future
//! normalization (a version of `is_not_found` that also matches
//! `kube::Error::Api(ErrorResponse { reason: "NotFound", .. })` for
//! servers that stamp the reason but not the code, or a version of
//! `is_conflict` that folds the `AlreadyExists`, `Conflict`, and
//! generation-mismatch reasons together) lands at THIS ONE substrate
//! owner and every downstream idempotent-write consumer inherits the
//! upgrade mechanically — no per-site edit at any of the FIVE listed
//! callers or at future consumers (an allocation delete-branch, a
//! pool-owned Process reap idempotent gate, a table-controller stale-
//! claim strip that must survive a race with cluster-side GC).
//!
//! ### `#[must_use]`
//!
//! Every consumer either drives a match-arm guard on the returned
//! bool or short-circuits a fallthrough branch on it. Dropping the
//! return means the predicate was computed for no observable reason —
//! the attribute surfaces that as a warning at every call site.

use kube::Error;

/// The kube error names an HTTP 409 Conflict response — a `create`
/// refused because the resource already exists, or a `patch` refused
/// because of an optimistic-concurrency generation mismatch.
///
/// See the module docs for the full callsite audit and the semantic-
/// axis rationale.
#[must_use]
pub fn is_conflict(err: &Error) -> bool {
    matches!(err, Error::Api(e) if e.code == 409)
}

/// The kube error names an HTTP 404 Not Found response — the K8s API
/// server has no resource with the given key.
///
/// See the module docs for the full callsite audit and the semantic-
/// axis rationale.
#[must_use]
pub fn is_not_found(err: &Error) -> bool {
    matches!(err, Error::Api(e) if e.code == 404)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kube::core::ErrorResponse;

    fn api_err(code: u16) -> Error {
        Error::Api(ErrorResponse {
            status: "Failure".into(),
            message: format!("test code {code}"),
            reason: match code {
                404 => "NotFound".into(),
                409 => "AlreadyExists".into(),
                _ => "Test".into(),
            },
            code,
        })
    }

    #[test]
    fn conflict_matches_only_409_api_variant() {
        // The 409-code pin — a regression that widened the predicate
        // to any non-2xx code, that swapped the equality-check for
        // an inequality (`!= 200`), or that dropped the `Api` variant
        // guard (matching on Auth / Discovery / other sub-variants
        // whose interior doesn't carry a `code` slot) would surface
        // HERE rather than as silent already-exists-branch drift at
        // every one of the FIVE consumer sites.
        assert!(is_conflict(&api_err(409)));
        assert!(!is_conflict(&api_err(200)));
        assert!(!is_conflict(&api_err(400)));
        assert!(!is_conflict(&api_err(404)));
        assert!(!is_conflict(&api_err(410)));
        assert!(!is_conflict(&api_err(500)));
    }

    #[test]
    fn not_found_matches_only_404_api_variant() {
        // The 404-code pin — sibling to the 409 pin above. A
        // regression that folded 404 and 410 (Gone) together, or that
        // aliased 404 to any client-error code, would surface HERE
        // rather than as silent already-gone-branch drift at the
        // watcher's delete-branch (which reads the arm as "the
        // allocation is not present, return 200 OK to the webhook").
        assert!(is_not_found(&api_err(404)));
        assert!(!is_not_found(&api_err(200)));
        assert!(!is_not_found(&api_err(400)));
        assert!(!is_not_found(&api_err(409)));
        assert!(!is_not_found(&api_err(410)));
        assert!(!is_not_found(&api_err(500)));
    }

    #[test]
    fn conflict_and_not_found_are_mutually_exclusive() {
        // Every kube::Error the predicates are asked about maps onto
        // at most ONE of the two semantics — 404 and 409 are distinct
        // HTTP status codes, and the K8s API server sends them for
        // distinct outcomes. Pin the mutual exclusivity so a future
        // normalization that widened the interior match on ONE
        // predicate can't silently start matching the OTHER's code
        // and start double-firing at every match with both arms.
        for code in [200u16, 400, 404, 409, 410, 500, 503] {
            let e = api_err(code);
            assert!(
                !(is_conflict(&e) && is_not_found(&e)),
                "conflict + not_found both fired for code {code}"
            );
        }
    }

    #[test]
    fn non_api_variants_return_false_for_both_predicates() {
        // The `Api`-variant guard is load-bearing — every other
        // `kube::Error` sub-variant (transport, auth, discovery, …)
        // has no `code` slot to inspect, so the predicate MUST return
        // `false` rather than panic or match by accident. Pin one of
        // the codeless sub-variants so a future refactor that swept
        // the `Api` guard out of the `matches!` shape (leaving only
        // the code arithmetic) surfaces HERE as a compile-time /
        // pattern-shape defect.
        let sd_err = kube::Error::LinesCodecMaxLineLengthExceeded;
        assert!(!is_conflict(&sd_err));
        assert!(!is_not_found(&sd_err));
    }
}
