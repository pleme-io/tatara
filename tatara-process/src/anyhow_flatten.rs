//! Substrate primitive over `anyhow::Result<T>` — the ONE substrate
//! owner of the `.map_err(|e| anyhow::anyhow!("<ctx>: {e}"))` wrap-
//! shape every reconciler consumer restates by hand at the
//! anyhow-returning → anyhow-returning display-prefix boundary. Peer
//! of [`crate::kube_error::KubeResultExt`] and
//! [`crate::hostname::HostnameResultExt`] on the flatten-wrap axis; the
//! three traits partition the display-prefix wrap space by underlying
//! error type — [`kube::Error`] on the K8s peer, [`crate::hostname::
//! HostnameError`] on the hostname peer, [`anyhow::Error`] on this
//! peer (the "already an anyhow error, we just want a static or
//! `format!`-composed slug in front of its `Display` output" case).
//!
//! Pre-lift the shape was hand-authored at FIVE `phase_machine.rs`
//! sites in `tatara-reconciler` past the ★★ PRIME-DIRECTIVE ≥ 2
//! duplication threshold, each restating the SAME closure — capture
//! an [`anyhow::Error`] returned by a downstream primitive
//! ([`crate::boundary::check_depends_on`],
//! [`crate::render::render_routing`],
//! [`crate::boundary::evaluate`],
//! [`crate::render::render_export_jobs`],
//! [`crate::ssapply::apply_owned`]), prepend a context slug identifying
//! which primitive faulted, delegate the tail to [`anyhow::Error`]'s
//! `Display` impl via the `{e}` slot — differing only in the context
//! slug prefix each callsite stamped.
//!
//! Post-lift each callsite reads
//! `<anyhow-returning-call>().await.flatten_ctx("<slug>")?` (or the
//! owned-`String` peer for consumers that compose the slug via
//! `format!`) and the wrap-shape lives at ONE substrate owner here.
//! The composed [`anyhow::Error`]'s `Display` is byte-identical to
//! the pre-lift chain (`format!("{ctx}: {e}")`, threading the source
//! [`anyhow::Error`]'s own `Display` verbatim into the `{e}` slot),
//! so operator-facing log output and any error-chain greps still
//! match bytewise. A regression that drifts the separator, swaps
//! the two slots, or wraps the source [`anyhow::Error`] with a
//! chain-form `source` (which would change `Display` output on the
//! `err` slot when downstream consumers format with `{e}` rather
//! than `{e:#}`) surfaces at the tests below rather than as silent
//! operator-facing drift across the five pre-lift consumers.
//!
//! ### Naming — `flatten_ctx`, not `anyhow::Context::context`
//!
//! Same discipline as [`crate::kube_error::KubeResultExt::kube_ctx`]
//! and [`crate::hostname::HostnameResultExt::hostname_ctx`] — but with
//! a more urgent motivation, because THIS trait operates on the SAME
//! `anyhow::Result<T>` type [`anyhow::Context::context`] takes.
//! `anyhow::Context::context` wraps the source in a chain (so
//! `Display` emits ONLY the context slug and callers reach the source
//! [`anyhow::Error`] via [`std::error::Error::source`] traversal, or
//! by formatting with the alternate `{:#}` specifier that walks the
//! chain), while this trait's `flatten_ctx` FLATTENS to a display-
//! prefix shape (`"<ctx>: <anyhow::Error display>"`) — the pre-lift
//! wire format every consumer's `tracing::error!(error = %e, ...)`
//! log line already encoded. Sharing the name (`context`) would let
//! a caller who has [`anyhow::Context`] in scope resolve to the
//! WRONG method (a chain-wrap instead of the display-prefix flatten)
//! and silently drop the underlying error detail from every
//! reconciler-error tracing span whose formatter interpolates `{e}`.
//!
//! ### Two flavors: `flatten_ctx` + `flatten_ctx_with`
//!
//! * [`FlattenCtxExt::flatten_ctx`] takes a `&'static str` context —
//!   the most common shape, matching every static-slug consumer
//!   (`"depends_on check"`, `"render routing"`, `"render export
//!   jobs"`, `"apply export job"`). Static binding keeps the
//!   compile-time contract that the context slug is a bare literal,
//!   no allocation, no dynamic content leaking into an error stream
//!   downstream operators grep on.
//! * [`FlattenCtxExt::flatten_ctx_with`] takes an owned [`String`]
//!   context — the escape hatch for the one dynamic-slug consumer
//!   (`format!("evaluate {:?}", c.kind)`, the `ConditionKind`
//!   variant name only known at runtime), matching the pre-lift
//!   shape where a `format!` composed the slug per-call.
//!
//! ### `#[must_use]`
//!
//! Every consumer threads the `?` short-circuit onto its handler's
//! `Result<_, anyhow::Error>` return — dropping the wrap swallows
//! the underlying failure entirely, which is never the intended
//! semantic at any of the five pre-lift consumers.
//!
//! Theory anchor: THEORY.md §VI.1 (generation over composition — the
//! anyhow-with-display-prefix wrap-shape recurred at five hand-
//! authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
//! trigger, and is lifted to ONE substrate owner here). THEORY.md
//! §II.1 invariant 5 (composition preserves proofs — a regression
//! that drifts the display-prefix separator or the byte-shape at
//! ONE site surfaces here at the substrate pin rather than as
//! silent operator-facing skew across every reconciler phase tick).

/// Substrate extension trait over `anyhow::Result<T>` — the ONE
/// substrate owner of the `.map_err(|e| anyhow::anyhow!("<ctx>: {e}"))`
/// display-prefix wrap-shape for consumers whose source error is
/// already `anyhow::Error`. See the module docs for the full callsite
/// audit + the naming rationale (why `flatten_ctx` and not
/// `anyhow::Context::context`).
pub trait FlattenCtxExt<T>: Sized {
    /// Wrap the source [`anyhow::Error`] (if any) with a static
    /// context prefix, producing an [`anyhow::Result`] whose error
    /// `Display` reads exactly `"<context>: <source display>"`.
    #[must_use = "an error wrap that isn't threaded via `?` swallows the underlying anyhow failure"]
    fn flatten_ctx(self, context: &'static str) -> anyhow::Result<T>;

    /// Owned-string peer of [`Self::flatten_ctx`] — the escape hatch
    /// for consumers that compose the context slug via `format!`
    /// (e.g. `format!("evaluate {:?}", c.kind)` where the tail is
    /// only known at runtime).
    #[must_use = "an error wrap that isn't threaded via `?` swallows the underlying anyhow failure"]
    fn flatten_ctx_with(self, context: String) -> anyhow::Result<T>;
}

impl<T> FlattenCtxExt<T> for anyhow::Result<T> {
    #[inline]
    fn flatten_ctx(self, context: &'static str) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::anyhow!("{context}: {e}"))
    }

    #[inline]
    fn flatten_ctx_with(self, context: String) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::anyhow!("{context}: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_err() -> anyhow::Error {
        anyhow::anyhow!("underlying primitive failed: bad slot 42")
    }

    // ─── FlattenCtxExt::flatten_ctx substrate pins ───────────────────
    //
    // Fail-before-pass-after granularity: the `FlattenCtxExt::flatten_ctx`
    // trait method did not exist before this commit, so each test below
    // fails to compile pre-lift. Post-lift they collectively pin the
    // display-prefix wrap-shape at ONE substrate owner — a regression
    // that drifts the separator, swaps the two slots, wraps the source
    // anyhow error with a chain-form `source` (which would change
    // `Display` output when downstream tracing formatters interpolate
    // `{e}` rather than the chain-walking `{e:#}`), or promotes the
    // pass-through arm to a synthesis (an empty `Ok(())`, a mutated
    // context slug) surfaces HERE rather than as silent operator-facing
    // skew across the five `phase_machine.rs` pre-lift consumers whose
    // log output already encoded the flat `"<ctx>: <anyhow display>"`
    // shape.

    #[test]
    fn flatten_ctx_static_str_context_matches_pre_lift_format_bytewise() {
        // Byte-shape parity pin: the wrap output of `flatten_ctx
        // ("<slug>")` MUST be `Display`-identical to the pre-lift
        // hand-authored `.map_err(|e| anyhow!("<slug>: {e}"))` chain.
        // A regression that inserted a separator character (`"<slug>::
        // <anyhow>"`), dropped the space after the colon, or swapped
        // the two slots (`"<anyhow>: <slug>"`) surfaces HERE rather
        // than as silent drift at every downstream log-output consumer.
        let raw: anyhow::Result<()> = Err(sample_err());
        let via_trait = raw.flatten_ctx("depends_on check").unwrap_err();
        let pre_lift = anyhow::anyhow!("depends_on check: {}", sample_err());
        assert_eq!(
            format!("{via_trait}"),
            format!("{pre_lift}"),
            "flatten_ctx wrap must be Display-identical to pre-lift anyhow! chain"
        );
    }

    #[test]
    fn flatten_ctx_ok_arm_is_a_pure_passthrough() {
        // Ok-arm invariant: `flatten_ctx` on `Ok(t)` MUST return
        // `Ok(t)` verbatim — no side-effect on the payload, no
        // synthesis of a context-tagged error, no allocation. Peer to
        // the Err-arm byte-shape pin; a regression that promoted the
        // Ok arm to ALWAYS produce a synthesis Error would silently
        // break every successful downstream primitive call in the
        // pre-lift consumer set.
        let raw: anyhow::Result<i32> = Ok(42);
        assert_eq!(raw.flatten_ctx("noop").unwrap(), 42);
    }

    #[test]
    fn flatten_ctx_with_owned_string_matches_pre_lift_format_bytewise() {
        // Owned-string peer's byte-shape pin — same discipline as the
        // static-`&str` peer above. Consumers that compose the context
        // slug via `format!` (e.g. `format!("evaluate {:?}", c.kind)`)
        // route through this method and inherit the SAME display-prefix
        // discipline as the static-slug peer, so mixing the two forms
        // across the reconciler's log stream never surfaces as a
        // format-string skew.
        let raw: anyhow::Result<()> = Err(sample_err());
        let dynamic_slug = format!("evaluate {:?}", "HelmReleaseReleased");
        let via_trait = raw.flatten_ctx_with(dynamic_slug.clone()).unwrap_err();
        let pre_lift = anyhow::anyhow!("{}: {}", dynamic_slug, sample_err());
        assert_eq!(
            format!("{via_trait}"),
            format!("{pre_lift}"),
            "flatten_ctx_with wrap must be Display-identical to pre-lift anyhow! chain"
        );
    }

    #[test]
    fn flatten_ctx_with_ok_arm_is_a_pure_passthrough() {
        // Ok-arm invariant on the owned-string peer — sibling to the
        // static-slug pin above. A regression that promoted the Ok arm
        // of the dynamic-slug peer to a synthesis while leaving the
        // static-slug peer's Ok arm passthrough would surface HERE as
        // an owned-string-peer-specific asymmetry rather than as silent
        // drift at the one `phase_machine::evaluate_conditions` consumer.
        let raw: anyhow::Result<&'static str> = Ok("condition satisfied");
        assert_eq!(
            raw.flatten_ctx_with("dynamic".to_string()).unwrap(),
            "condition satisfied"
        );
    }

    #[test]
    fn flatten_ctx_static_and_owned_peers_produce_identical_output_for_the_same_slug() {
        // Cross-peer coherence pin: given the SAME context slug via
        // both peers (a `&'static str` passed to `flatten_ctx` and the
        // owned `String` produced by `.to_string()` passed to
        // `flatten_ctx_with`), the wrapped `anyhow::Error` MUST have
        // byte-identical `Display` output. A regression that drifted
        // one peer's format string away from the other would surface
        // HERE rather than as silent operator-facing skew between the
        // four static-slug consumers and the one `format!`-slug
        // consumer in the same log stream.
        let slug = "render routing";
        let a: anyhow::Result<()> = Err(sample_err());
        let b: anyhow::Result<()> = Err(sample_err());
        assert_eq!(
            format!("{}", a.flatten_ctx(slug).unwrap_err()),
            format!("{}", b.flatten_ctx_with(slug.to_string()).unwrap_err()),
            "static-str and owned-string peers must produce identical Display output"
        );
    }

    #[test]
    fn flatten_ctx_threads_the_underlying_anyhow_display_verbatim() {
        // Display-tail invariant: the wrapped `anyhow::Error`'s
        // `Display` output MUST contain the source `anyhow::Error`'s
        // own `Display` output verbatim as the tail past `"<ctx>: "`.
        // A regression that inserted a normalization (uppercase, JSON
        // encoding, truncation) between the composed `{e}` slot and
        // the underlying `Display` impl would surface HERE rather
        // than as silent underlying-error-detail loss across the
        // reconciler's error stream.
        let underlying_display = format!("{}", sample_err());
        let raw: anyhow::Result<()> = Err(sample_err());
        let wrapped = raw.flatten_ctx("apply export job").unwrap_err();
        let wrapped_display = format!("{wrapped}");
        assert!(
            wrapped_display.ends_with(&underlying_display),
            "wrapped Display `{wrapped_display}` must end with underlying anyhow Display `{underlying_display}`"
        );
        assert!(
            wrapped_display.starts_with("apply export job: "),
            "wrapped Display `{wrapped_display}` must start with `\"<ctx>: \"`"
        );
    }
}
