//! Substrate primitive over `Result<T, E>` for any `E: `[`std::fmt::
//! Display`] — the ONE substrate owner of the generic `.map_err(|e|
//! anyhow::anyhow!("<ctx>: {e}"))` display-prefix wrap-shape for
//! consumers whose source error is a bare `Display` type NOT already
//! covered by a per-error-type flatten-wrap peer.
//!
//! Peer of the type-specific flatten-wrap trait trio already in this
//! crate on the same display-prefix wrap axis, partitioning the space
//! by SPECIFICITY:
//!
//! * [`crate::kube_error::KubeResultExt::kube_ctx`] — the specialized
//!   peer for `Result<T, kube::Error>`, kept because
//!   [`kube::Error`]'s `Display` composes the request URI + status
//!   line in a shape every reconciler-side callsite already greps on.
//! * [`crate::hostname::HostnameResultExt::hostname_ctx`] — the
//!   specialized peer for `Result<T, `[`crate::hostname::HostnameError`]`>`,
//!   kept because the [`thiserror`]-derived `Display` output matches
//!   the pre-lift hand-authored render_routing log stream verbatim.
//! * [`crate::anyhow_flatten::FlattenCtxExt::flatten_ctx`] — the
//!   specialized peer for `anyhow::Result<T>`, kept because it
//!   collides with [`anyhow::Context::context`]'s naming so the
//!   distinct-method-name discipline (flatten-prefix vs. chain-wrap
//!   semantics) is load-bearing at every phase-machine callsite.
//! * [`ErrCtxExt::err_ctx`] (this trait) — the generic fallback for
//!   any `E: Display` NOT covered by the three specialized peers,
//!   so a new consumer whose source error is a fresh
//!   [`crate::tagged_union::declare_tagged_union_error`]-derived
//!   variant (e.g. [`crate::export::ArtifactError`],
//!   [`crate::intent::IntentError`],
//!   [`crate::lifetime::LifetimeError`]) reaches the display-prefix
//!   wrap-shape mechanically at ONE substrate owner instead of
//!   opening a fourth per-error-type peer trait for every fresh
//!   [`thiserror`]-derived enum.
//!
//! Pre-lift the shape was hand-authored at TWO
//! `tatara-export-worker/src/main.rs` sites past the ★★ PRIME-DIRECTIVE
//! ≥ 2 duplication threshold, both restating the SAME closure —
//! capture an [`crate::export::ArtifactError`] returned by the
//! substrate primitive [`crate::export::ArtifactSource::variant`],
//! prepend the identical static context slug `"source"`, delegate
//! the tail to [`std::fmt::Display`] via the `{e}` slot — differing
//! in NOTHING but their line numbers. Post-lift both callsites read
//! `spec.source.variant().err_ctx("source")?` and the wrap-shape
//! lives at ONE substrate owner here.
//!
//! ### Naming — `err_ctx`, not `context`
//!
//! Same discipline as the three specialized peers: the method name
//! `err_ctx` is deliberately DISTINCT from [`anyhow::Context::context`]
//! so a caller with [`anyhow::Context`] in scope can never resolve to
//! the wrong method (which chain-wraps rather than display-prefix-
//! flattens, and would silently drop the underlying error detail from
//! every downstream `tracing::error!(error = %e, ...)` log line whose
//! formatter interpolates `{e}` rather than the chain-walking `{e:#}`).
//!
//! ### Two flavors: `err_ctx` + `err_ctx_with`
//!
//! * [`ErrCtxExt::err_ctx`] takes a `&'static str` context — the
//!   most common shape (`"source"` at the two export-worker
//!   callsites). Static binding keeps the compile-time contract that
//!   the context slug is a bare literal, no allocation, no dynamic
//!   content leaking into an error stream downstream operators grep
//!   on.
//! * [`ErrCtxExt::err_ctx_with`] takes an owned [`String`] context —
//!   the escape hatch for future consumers that compose the slug via
//!   [`format!`] (e.g. a dynamic per-variant-name slug the way
//!   [`crate::anyhow_flatten::FlattenCtxExt::flatten_ctx_with`]
//!   already services for `phase_machine::evaluate_conditions`).
//!
//! ### `#[must_use]`
//!
//! Every consumer threads the `?` short-circuit onto its handler's
//! `anyhow::Result<_>` return — dropping the wrap swallows the
//! underlying failure entirely, which is never the intended
//! semantic.
//!
//! Theory anchor: THEORY.md §VI.1 (generation over composition — the
//! generic display-prefix wrap-shape recurred at two byte-identical
//! hand-authored sites in `tatara-export-worker/src/main.rs` past
//! the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted to
//! ONE substrate owner here). THEORY.md §II.1 invariant 5
//! (composition preserves proofs — a regression that drifts the
//! display-prefix separator or the byte-shape surfaces here at the
//! substrate pin rather than as silent operator-facing skew across
//! every downstream `err_ctx` consumer).

/// Substrate extension trait over `Result<T, E>` for any `E: `
/// [`std::fmt::Display`] — the ONE substrate owner of the generic
/// `.map_err(|e| anyhow::anyhow!("<ctx>: {e}"))` display-prefix
/// wrap-shape for consumers whose source error is a bare `Display`
/// type NOT already covered by a per-error-type flatten-wrap peer.
/// See the module docs for the specialized-peer partition + the
/// naming rationale (why `err_ctx` and not `context`).
pub trait ErrCtxExt<T>: Sized {
    /// Wrap the source error (if any) with a static context prefix,
    /// producing an [`anyhow::Result`] whose error `Display` reads
    /// exactly `"<context>: <source display>"`.
    #[must_use = "an error wrap that isn't threaded via `?` swallows the underlying failure"]
    fn err_ctx(self, context: &'static str) -> anyhow::Result<T>;

    /// Owned-string peer of [`Self::err_ctx`] — the escape hatch for
    /// consumers that compose the context slug via [`format!`].
    #[must_use = "an error wrap that isn't threaded via `?` swallows the underlying failure"]
    fn err_ctx_with(self, context: String) -> anyhow::Result<T>;
}

impl<T, E> ErrCtxExt<T> for Result<T, E>
where
    E: std::fmt::Display,
{
    #[inline]
    fn err_ctx(self, context: &'static str) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::anyhow!("{context}: {e}"))
    }

    #[inline]
    fn err_ctx_with(self, context: String) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::anyhow!("{context}: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A representative bare-`Display` error type — matches the shape
    // of [`crate::export::ArtifactError`] (the concrete
    // [`thiserror`]-derived error whose two pre-lift callsites drove
    // this lift) without pulling the whole export module into the
    // test surface. A regression that promotes the trait bound to
    // e.g. `E: std::error::Error` would surface HERE (this type
    // doesn't impl `Error`) rather than as a silent narrowing of the
    // substrate's admissibility surface.
    #[derive(Debug)]
    struct DisplayErr(&'static str);
    impl std::fmt::Display for DisplayErr {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }

    // ─── ErrCtxExt::err_ctx substrate pins ───────────────────────────
    //
    // Fail-before-pass-after granularity: the `ErrCtxExt::err_ctx`
    // trait method did not exist before this commit, so each test
    // below fails to compile pre-lift. Post-lift they collectively
    // pin the display-prefix wrap-shape at ONE substrate owner — a
    // regression that drifts the separator, swaps the two slots,
    // narrows the trait bound (`E: Error` in place of `E: Display`,
    // ruling out today's `ArtifactError` and similar bare-Display
    // enums that don't derive `Error`), or promotes the pass-through
    // arm to a synthesis surfaces HERE rather than as silent
    // operator-facing skew across the two export-worker pre-lift
    // consumers whose log output already encoded the flat
    // `"source: <ArtifactError display>"` shape.

    #[test]
    fn err_ctx_static_str_context_matches_pre_lift_format_bytewise() {
        // Byte-shape parity pin: the wrap output of
        // `err_ctx("<slug>")` MUST be `Display`-identical to the
        // pre-lift hand-authored `.map_err(|e| anyhow!("<slug>:
        // {e}"))` chain. A regression that inserted a separator
        // character (`"<slug>:: <err>"`), dropped the space after
        // the colon, or swapped the two slots (`"<err>: <slug>"`)
        // surfaces HERE rather than as silent drift at every
        // downstream log-output consumer.
        let raw: Result<(), DisplayErr> = Err(DisplayErr("bad slot"));
        let via_trait = raw.err_ctx("source").unwrap_err();
        assert_eq!(format!("{via_trait}"), "source: bad slot");
    }

    #[test]
    fn err_ctx_ok_arm_is_a_pure_passthrough() {
        // Ok-arm invariant: `err_ctx` on `Ok(t)` MUST return `Ok(t)`
        // verbatim — no side-effect on the payload, no synthesis of
        // a context-tagged error. Peer to the Err-arm byte-shape
        // pin; a regression that promoted the Ok arm to ALWAYS
        // produce a synthesis Error would silently break every
        // successful downstream primitive call in the pre-lift
        // consumer set.
        let raw: Result<i32, DisplayErr> = Ok(42);
        assert_eq!(raw.err_ctx("noop").unwrap(), 42);
    }

    #[test]
    fn err_ctx_with_owned_string_matches_pre_lift_format_bytewise() {
        // Owned-string peer's byte-shape pin — same discipline as
        // the static-`&str` peer above. Future consumers that
        // compose the context slug via `format!` route through this
        // method and inherit the SAME display-prefix discipline as
        // the static-slug peer, so mixing the two forms across a
        // consumer's log stream never surfaces as a format-string
        // skew.
        let raw: Result<(), DisplayErr> = Err(DisplayErr("bad slot"));
        let dynamic_slug = format!("evaluate {:?}", "SomeVariant");
        let via_trait = raw.err_ctx_with(dynamic_slug.clone()).unwrap_err();
        assert_eq!(format!("{via_trait}"), format!("{dynamic_slug}: bad slot"));
    }

    #[test]
    fn err_ctx_with_ok_arm_is_a_pure_passthrough() {
        // Ok-arm invariant on the owned-string peer — sibling to
        // the static-slug pin above.
        let raw: Result<&'static str, DisplayErr> = Ok("variant resolved");
        assert_eq!(
            raw.err_ctx_with("dynamic".to_string()).unwrap(),
            "variant resolved"
        );
    }

    #[test]
    fn err_ctx_static_and_owned_peers_produce_identical_output_for_the_same_slug() {
        // Cross-peer coherence pin: given the SAME context slug via
        // both peers, the wrapped [`anyhow::Error`] MUST have
        // byte-identical `Display` output.
        let slug = "source";
        let a: Result<(), DisplayErr> = Err(DisplayErr("bad slot"));
        let b: Result<(), DisplayErr> = Err(DisplayErr("bad slot"));
        assert_eq!(
            format!("{}", a.err_ctx(slug).unwrap_err()),
            format!("{}", b.err_ctx_with(slug.to_string()).unwrap_err()),
            "static-str and owned-string peers must produce identical Display output"
        );
    }

    #[test]
    fn err_ctx_threads_the_underlying_display_verbatim() {
        // Display-tail invariant: the wrapped [`anyhow::Error`]'s
        // `Display` output MUST contain the source error's own
        // `Display` output verbatim as the tail past `"<ctx>: "`.
        let underlying_display = format!("{}", DisplayErr("bad slot"));
        let raw: Result<(), DisplayErr> = Err(DisplayErr("bad slot"));
        let wrapped = raw.err_ctx("source").unwrap_err();
        let wrapped_display = format!("{wrapped}");
        assert!(
            wrapped_display.ends_with(&underlying_display),
            "wrapped Display `{wrapped_display}` must end with underlying Display `{underlying_display}`"
        );
        assert!(
            wrapped_display.starts_with("source: "),
            "wrapped Display `{wrapped_display}` must start with `\"<ctx>: \"`"
        );
    }

    #[test]
    fn err_ctx_admits_thiserror_derived_tagged_union_error() {
        // Substrate coverage pin: the trait's `E: Display` bound
        // MUST admit the concrete error type both pre-lift
        // export-worker callsites captured — a
        // [`crate::tagged_union::declare_tagged_union_error`]-
        // derived variant whose `Display` is [`thiserror`]-generated.
        // This test exercises `ArtifactError` specifically (the two
        // pre-lift consumers' source error) so a regression that
        // dropped its `Display` impl, narrowed the trait bound, or
        // otherwise made the primitive inapplicable to the very
        // callsites it was opened for surfaces HERE.
        use crate::export::{ArtifactError, ARTIFACT_KIND_LIST};
        let raw: Result<(), ArtifactError> = Err(ArtifactError::Empty(ARTIFACT_KIND_LIST));
        let wrapped = raw.err_ctx("source").unwrap_err();
        let wrapped_display = format!("{wrapped}");
        assert!(
            wrapped_display.starts_with("source: "),
            "wrapped Display `{wrapped_display}` must start with `\"source: \"`"
        );
        assert!(
            wrapped_display.contains(ARTIFACT_KIND_LIST),
            "wrapped Display `{wrapped_display}` must thread ArtifactError's Display body verbatim"
        );
    }

    #[test]
    fn err_ctx_agrees_with_flatten_ctx_on_anyhow_result() {
        // Cross-substrate coherence pin: on the specific input shape
        // `anyhow::Result<T>`, the generic `err_ctx` and the
        // specialized peer
        // [`crate::anyhow_flatten::FlattenCtxExt::flatten_ctx`] MUST
        // produce byte-identical `Display` output. A regression that
        // drifted either surface would surface HERE rather than as
        // silent operator-facing skew between consumers migrated
        // onto the generic and consumers still routed through the
        // specialized peer.
        use crate::anyhow_flatten::FlattenCtxExt;
        let a: anyhow::Result<()> = Err(anyhow::anyhow!("underlying failure"));
        let b: anyhow::Result<()> = Err(anyhow::anyhow!("underlying failure"));
        assert_eq!(
            format!("{}", a.err_ctx("source").unwrap_err()),
            format!("{}", b.flatten_ctx("source").unwrap_err()),
            "generic err_ctx and specialized flatten_ctx must agree on anyhow::Result"
        );
    }
}
