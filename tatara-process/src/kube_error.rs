//! Substrate primitives over [`kube::Error`] — the semantic layer
//! every controller's `match … { Err(kube::Error::Api(e)) if e.code
//! == <N> => … }` guard AND every consumer's
//! `.map_err(|e| anyhow::anyhow!("<ctx>: {e}"))?` error-wrap restates
//! by hand pre-lift.
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
//! Plus the [`KubeResultExt`] extension trait on `Result<T, kube::Error>`
//! that owns the display-prefix wrap the phase-machine / signal-effect
//! / pool-reconciler consumers thread every K8s round-trip through into
//! their anyhow-returning handler:
//!
//! * [`KubeResultExt::kube_ctx`] — attach a `&'static str` context slug
//!   to a `Result<_, kube::Error>` and get an
//!   [`anyhow::Result`] whose `Display` reads
//!   `"<ctx>: <kube::Error display>"` (byte-identical to the pre-lift
//!   `.map_err(|e| anyhow!("<ctx>: {e}"))` chain).
//! * [`KubeResultExt::kube_ctx_with`] — owned-`String` peer for
//!   consumers that compose the slug via `format!` at runtime.
//!
//! See the trait's own docstring for the full consumer inventory + the
//! naming rationale (why `kube_ctx` and not `anyhow::Context::context`).
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

/// Substrate extension trait over `Result<T, kube::Error>` — the ONE
/// substrate owner of the `.map_err(|e| anyhow::anyhow!("<ctx>: {e}"))`
/// wrap-shape every reconciler consumer restates by hand at the
/// K8s round-trip → anyhow error boundary.
///
/// Pre-lift this shape was hand-authored at 25+ sites across
/// `tatara-reconciler` + `tatara-pool-reconciler` — every consumer
/// that awaited a `Result<_, kube::Error>` and needed to thread it
/// into an anyhow-returning phase-handler / signal-effect handler /
/// pool-reconciler tick. Each site restated the SAME closure — capture
/// a [`kube::Error`], prepend a static (or `format!`-owned) context
/// slug, delegate the tail to [`kube::Error`]'s `Display` impl via the
/// `{e}` slot — differing only in the context slug prefix each
/// handler stamped. The consumer inventory spans the full reconciler
/// phase machine (`install finalizer`, `patch status`, `ensure
/// ProcessTable`, `bump nextSequence`, `patch pid`, every phase
/// transition wrap, `patch fluxResources`, `patch postconditions`,
/// `patch attestation`, `list export jobs`, `list processes`), every
/// signal-effect handler in `signals.rs` (`transition via signal`,
/// `force attest`, `suspend`, `resume`, `remediate`), and the pool-
/// reconciler's controller entry points.
///
/// Post-lift each callsite reads
/// `<kube-returning-call>().await.kube_ctx("<slug>")?` and the
/// wrap-shape lives at ONE substrate owner here. The composed
/// [`anyhow::Error`]'s `Display` is byte-identical to the pre-lift
/// chain (`format!("{ctx}: {e}")`, threading the [`kube::Error`]'s
/// own `Display` verbatim into the `{e}` slot), so operator-facing
/// log output and error-chain greps still match bytewise. A regression
/// that drifts the separator, swaps the two slots, or wraps the
/// [`kube::Error`] with a chain-form `source` (which would change
/// `Display` output on the `err` slot) surfaces at
/// [`tests::kube_ctx_static_str_context_matches_pre_lift_format_bytewise`]
/// rather than as silent operator-facing drift across every one of
/// the 25+ pre-lift consumers.
///
/// ### Two flavors: `kube_ctx` + `kube_ctx_with`
///
/// * [`Self::kube_ctx`] takes a `&'static str` context — the most
///   common shape, matching every static-slug consumer
///   (`"install finalizer"`, `"patch attestation"`, etc.). Static
///   binding keeps the compile-time contract that the context slug
///   is a bare literal, no allocation, no dynamic content leaking
///   into an error stream downstream operators grep on.
/// * [`Self::kube_ctx_with`] takes an owned [`String`] context — the
///   escape hatch for the two dynamic-slug consumers
///   (`format!("patch (releasing→{next})")`, the `next` variant
///   name is only known at runtime), matching the pre-lift shape
///   where a `format!` composed the slug per-call.
///
/// Naming — `kube_ctx` rather than the anyhow crate's `.context(...)` —
/// is deliberate. `anyhow::Context::context` wraps the source in a
/// chain (so `Display` emits only the context slug and callers reach
/// the `kube::Error` via [`std::error::Error::source`] traversal), while
/// this trait's `kube_ctx` FLATTENS to a display-prefix shape
/// (`"<ctx>: <kube::Error display>"`) — the pre-lift wire format every
/// consumer's log output already encoded. Sharing the name would let
/// a caller who has `anyhow::Context` in scope resolve to the WRONG
/// method (a chain-wrap instead of the display-prefix flatten) and
/// silently change every operator log message.
///
/// The `#[must_use]` attribute rides through from the trait method —
/// dropping the return of a K8s round-trip wrap means the error is
/// swallowed entirely, which is never the intended semantic at any of
/// the 25+ pre-lift consumers (each threads the `?` short-circuit onto
/// its handler's `Result<Action, anyhow::Error>` return).
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// KubeError → anyhow-with-display-prefix wrap-shape recurred at 25+
/// hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// trigger, and is lifted to ONE substrate owner here). THEORY.md
/// §II.1 invariant 5 (composition preserves proofs — a regression
/// that drifts the display-prefix separator or the byte-shape at ONE
/// site surfaces here at the substrate pin rather than as silent
/// operator-facing skew across every reconciler / signal / pool tick).
pub trait KubeResultExt<T>: Sized {
    /// Wrap the `kube::Error` (if any) with a static context prefix,
    /// producing an [`anyhow::Result`] whose error `Display` reads
    /// exactly `"<context>: <kube::Error display>"`.
    #[must_use = "an error wrap that isn't threaded via `?` swallows the K8s round-trip failure"]
    fn kube_ctx(self, context: &'static str) -> anyhow::Result<T>;

    /// Owned-string peer of [`Self::kube_ctx`] — the escape hatch for
    /// consumers that compose the context slug via `format!` (e.g.
    /// `format!("patch (releasing→{next})")` where the tail is only
    /// known at runtime).
    #[must_use = "an error wrap that isn't threaded via `?` swallows the K8s round-trip failure"]
    fn kube_ctx_with(self, context: String) -> anyhow::Result<T>;
}

impl<T> KubeResultExt<T> for Result<T, Error> {
    #[inline]
    fn kube_ctx(self, context: &'static str) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::anyhow!("{context}: {e}"))
    }

    #[inline]
    fn kube_ctx_with(self, context: String) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::anyhow!("{context}: {e}"))
    }
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

    // ─── KubeResultExt::kube_ctx substrate pins ──────────────────────
    //
    // Fail-before-pass-after granularity: the `KubeResultExt::kube_ctx`
    // trait method did not exist before this commit, so each test
    // below fails to compile pre-lift. Post-lift they collectively pin
    // the display-prefix wrap-shape at ONE substrate owner — a
    // regression that drifts the separator, swaps the two slots, wraps
    // the `kube::Error` with a chain-form `source`, or promotes the
    // pass-through arm to a synthesis (an empty `Ok(())`, a mutated
    // context slug) surfaces HERE rather than as silent operator-facing
    // skew across the 25+ pre-lift consumers whose log output already
    // encoded the flat `"<ctx>: <kube display>"` shape.

    #[test]
    fn kube_ctx_static_str_context_matches_pre_lift_format_bytewise() {
        // Byte-shape parity pin: the wrap output of `kube_ctx("<slug>")`
        // MUST be `Display`-identical to the pre-lift hand-authored
        // `.map_err(|e| anyhow!("<slug>: {e}"))` chain. A regression
        // that inserted a separator character (`"<slug>:: <kube>"`),
        // dropped the space after the colon, or swapped the two slots
        // (`"<kube>: <slug>"`) surfaces HERE rather than as silent
        // drift at every downstream log-output consumer.
        let raw: Result<(), Error> = Err(api_err(404));
        let via_trait = raw.kube_ctx("install finalizer").unwrap_err();
        let pre_lift = anyhow::anyhow!("install finalizer: {}", api_err(404));
        assert_eq!(
            format!("{via_trait}"),
            format!("{pre_lift}"),
            "kube_ctx wrap must be Display-identical to pre-lift anyhow! chain"
        );
    }

    #[test]
    fn kube_ctx_ok_arm_is_a_pure_passthrough() {
        // Ok-arm invariant: `kube_ctx` on `Ok(t)` MUST return `Ok(t)`
        // verbatim — no side-effect on the payload, no synthesis of a
        // context-tagged error, no allocation. Peer to the Err-arm
        // byte-shape pin; a regression that promoted the Ok arm to
        // ALWAYS produce a synthesis Error would silently break every
        // successful K8s round-trip in the pre-lift consumer set.
        let raw: Result<i32, Error> = Ok(42);
        assert_eq!(raw.kube_ctx("noop").unwrap(), 42);
    }

    #[test]
    fn kube_ctx_with_owned_string_matches_pre_lift_format_bytewise() {
        // Owned-string peer's byte-shape pin — same discipline as the
        // static-`&str` peer above. Consumers that compose the context
        // slug via `format!` (e.g. `format!("patch (releasing→{next})")`)
        // route through this method and inherit the SAME display-prefix
        // discipline as the static-slug peer, so mixing the two forms
        // across the reconciler's log stream never surfaces as a
        // format-string skew.
        let raw: Result<(), Error> = Err(api_err(409));
        let dynamic_slug = format!("patch (releasing→{})", "Exiting");
        let via_trait = raw.kube_ctx_with(dynamic_slug.clone()).unwrap_err();
        let pre_lift = anyhow::anyhow!("{}: {}", dynamic_slug, api_err(409));
        assert_eq!(
            format!("{via_trait}"),
            format!("{pre_lift}"),
            "kube_ctx_with wrap must be Display-identical to pre-lift anyhow! chain"
        );
    }

    #[test]
    fn kube_ctx_static_and_owned_peers_produce_identical_output_for_the_same_slug() {
        // Cross-peer coherence pin: given the SAME context slug via
        // both peers (a `&'static str` passed to `kube_ctx` and the
        // owned `String` produced by `.to_string()` passed to
        // `kube_ctx_with`), the wrapped `anyhow::Error` MUST have
        // byte-identical `Display` output. A regression that drifted
        // one peer's format string away from the other would surface
        // HERE rather than as silent operator-facing skew between
        // static-slug consumers and format!-slug consumers in the
        // same log stream.
        let slug = "patch attestation";
        let a: Result<(), Error> = Err(api_err(500));
        let b: Result<(), Error> = Err(api_err(500));
        assert_eq!(
            format!("{}", a.kube_ctx(slug).unwrap_err()),
            format!("{}", b.kube_ctx_with(slug.to_string()).unwrap_err()),
            "static-str and owned-string peers must produce identical Display output"
        );
    }

    #[test]
    fn kube_ctx_threads_the_underlying_kube_error_display_verbatim() {
        // Display-tail invariant: the wrapped `anyhow::Error`'s
        // `Display` output MUST contain the `kube::Error`'s own
        // `Display` output verbatim as the tail past `"<ctx>: "`.
        // A regression that inserted a normalization (uppercase, JSON
        // encoding, truncation) between the composed `{e}` slot and
        // the underlying `Display` impl would surface HERE rather
        // than as silent K8s-error-detail loss across the reconciler's
        // error stream.
        let underlying = api_err(404);
        let underlying_display = format!("{underlying}");
        let raw: Result<(), Error> = Err(underlying);
        let wrapped = raw.kube_ctx("list processes").unwrap_err();
        let wrapped_display = format!("{wrapped}");
        assert!(
            wrapped_display.ends_with(&underlying_display),
            "wrapped Display `{wrapped_display}` must end with underlying kube Display `{underlying_display}`"
        );
        assert!(
            wrapped_display.starts_with("list processes: "),
            "wrapped Display `{wrapped_display}` must start with `\"<ctx>: \"`"
        );
    }
}
