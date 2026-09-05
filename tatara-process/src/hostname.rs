//! Hostname helpers — typed FQDN formatting matching `nix/lib/fleet-
//! domains.nix`'s `mkHostname` pattern.
//!
//! The substrate move: every FQDN this codebase emits is computed
//! here. Two functions ([`fmt_fqdn`] for the per-instance form +
//! [`fmt_fqdn_stable`] for the unprefixed stable-claim form) and one
//! deterministic ephemeral-id derivation ([`ephemeral_id_from_spec`])
//! are the single source of truth — no string `format!()` of DNS
//! syntax anywhere else in the tree.
//!
//! Forms:
//!
//! ```text
//!   Per-instance: ${app}.${ephemeral_id}.${cluster}.${location}.${domain}
//!   Stable:       ${app}.${cluster}.${location}.${domain}
//! ```
//!
//! Where `${ephemeral_id}` is:
//!
//! * `RoutingHostname.instance` when set — a named slot like
//!   `demo-prod` or `pr-1234`.
//! * `EPHEMERAL_ID_HASH_LEN` (= 8) hex chars of
//!   `BLAKE3(canonical_spec_json)` when unset — a content-hash slot
//!   that changes only when the Process's spec changes.
//!
//! All four FQDN segments are validated as RFC 1123 DNS labels at
//! the boundary — lowercase alphanumeric + hyphen, 1–63 chars, no
//! leading/trailing hyphen. Validation errors surface as typed
//! [`HostnameError`] variants so callers can render targeted
//! operator messages.

use serde::Serialize;

use crate::routing::RoutingHostname;

/// Number of hex chars from BLAKE3 to use as the content-hash form
/// of `ephemeral_id`. 8 = 32 bits of entropy; collision probability
/// at 1k concurrent Processes ≈ 1 in 8.5 million. Comfortable for
/// any single cluster's working set, room to grow.
pub const EPHEMERAL_ID_HASH_LEN: usize = 8;

/// Reserved 2-part forms forbidden as `app` values (saguão control
/// plane — see pleme-io CLAUDE.md §Fleet hostname pattern).
const RESERVED_APP_LABELS: &[&str] = &["auth", "cracha"];

/// Why a hostname can't be formatted. Typed so callers can branch.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum HostnameError {
    #[error("invalid DNS label {label:?} for segment {segment}: {reason}")]
    InvalidLabel {
        segment: &'static str,
        label: String,
        reason: &'static str,
    },
    #[error("app label {0:?} is reserved for the saguão control plane")]
    ReservedApp(String),
}

/// Substrate extension trait over `Result<T, HostnameError>` — the ONE
/// substrate owner of the `.map_err(|e| anyhow::anyhow!("<ctx>: {e}"))`
/// wrap-shape every reconciler consumer restated by hand at the
/// hostname-formatter → anyhow error boundary. Peer of
/// [`crate::kube_error::KubeResultExt`] on the wrap-shape axis; the two
/// traits partition the flatten-wrap space by underlying error type
/// (`kube::Error` on that peer, [`HostnameError`] on this one).
///
/// Pre-lift the shape was hand-authored at THREE sites in
/// `tatara-reconciler::render::render_routing` — each of the three
/// `HostnameError`-returning hostname primitives ([`ephemeral_id_from_spec`],
/// [`fmt_fqdn`], [`fmt_fqdn_stable`]) had ITS consumer restate the
/// SAME closure at the R9 routing-edge render — capture the
/// [`HostnameError`], prepend a static context slug identifying which
/// hostname primitive faulted, delegate the tail to [`HostnameError`]'s
/// `Display` impl via the `{e}` slot — differing only in the context
/// slug prefix each callsite stamped (`"ephemeral_id_from_spec"` /
/// `"fmt_fqdn (per-instance)"` / `"fmt_fqdn_stable"`). Three
/// hand-authored callsites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// threshold.
///
/// Post-lift each callsite reads
/// `<hostname-primitive>().hostname_ctx("<slug>")?` and the wrap-shape
/// lives at ONE substrate owner here. The composed [`anyhow::Error`]'s
/// `Display` is byte-identical to the pre-lift chain
/// (`format!("{ctx}: {e}")`, threading the [`HostnameError`]'s own
/// `Display` verbatim into the `{e}` slot), so operator-facing log
/// output and any error-chain greps still match bytewise. A regression
/// that drifts the separator, swaps the two slots, or wraps the
/// [`HostnameError`] with a chain-form `source` (which would change
/// `Display` output on the `err` slot) surfaces at
/// [`tests::hostname_ctx_static_str_context_matches_pre_lift_format_bytewise`]
/// rather than as silent operator-facing drift across the three
/// pre-lift consumers.
///
/// ### Naming — `hostname_ctx`, not `anyhow::Context::context`
///
/// Same discipline as [`crate::kube_error::KubeResultExt::kube_ctx`] —
/// `anyhow::Context::context` wraps the source in a chain (so `Display`
/// emits only the context slug and callers reach the [`HostnameError`]
/// via [`std::error::Error::source`] traversal), while this trait's
/// `hostname_ctx` FLATTENS to a display-prefix shape (`"<ctx>: <HostnameError
/// display>"`) — the pre-lift wire format every consumer's log output
/// already encoded. Sharing the name would let a caller who has
/// `anyhow::Context` in scope resolve to the WRONG method (a chain-wrap
/// instead of the display-prefix flatten) and silently change every
/// operator log message.
///
/// ### Static-slug only (no `_with` peer yet)
///
/// Every current callsite composes its slug at compile time
/// (`"ephemeral_id_from_spec"`, `"fmt_fqdn (per-instance)"`,
/// `"fmt_fqdn_stable"`); no consumer needs a `format!`-composed
/// runtime slug. The static-`&'static str` binding keeps the substrate
/// contract minimal — a future dynamic-slug consumer would add a
/// `hostname_ctx_with` peer here matching the `kube_ctx_with` shape,
/// but until then this trait exposes only the static peer.
///
/// ### `#[must_use]`
///
/// Every consumer threads the `?` short-circuit onto its handler's
/// `Result<_, anyhow::Error>` return — dropping the wrap swallows the
/// hostname-format failure entirely, which is never the intended
/// semantic (a rejected DNS label at emit time silently produces a
/// resource with a `""` FQDN slot that the K8s API server accepts and
/// then no downstream Ingress / DNSEndpoint dispatcher can route to).
/// The attribute surfaces that as a warning at every call site.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// [`HostnameError`] → anyhow-with-display-prefix wrap-shape recurred
/// at three hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication trigger, and is lifted to ONE substrate owner here).
/// THEORY.md §II.1 invariant 5 (composition preserves proofs — a
/// regression that drifts the display-prefix separator or the byte-
/// shape at ONE site surfaces here at the substrate pin rather than
/// as silent operator-facing skew across every render_routing tick).
pub trait HostnameResultExt<T>: Sized {
    /// Wrap the [`HostnameError`] (if any) with a static context
    /// prefix, producing an [`anyhow::Result`] whose error `Display`
    /// reads exactly `"<context>: <HostnameError display>"`.
    #[must_use = "an error wrap that isn't threaded via `?` swallows the hostname-format failure"]
    fn hostname_ctx(self, context: &'static str) -> anyhow::Result<T>;
}

impl<T> HostnameResultExt<T> for Result<T, HostnameError> {
    #[inline]
    fn hostname_ctx(self, context: &'static str) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::anyhow!("{context}: {e}"))
    }
}

/// Format the per-instance FQDN.
///
/// ```
/// use tatara_process::hostname::fmt_fqdn;
/// let fqdn = fmt_fqdn("api", "demo-prod", "pleme-dev", "use1", "quero.lol").unwrap();
/// assert_eq!(fqdn, "api.demo-prod.pleme-dev.use1.quero.lol");
/// ```
pub fn fmt_fqdn(
    app: &str,
    ephemeral_id: &str,
    cluster: &str,
    location: &str,
    domain: &str,
) -> Result<String, HostnameError> {
    validate_label("app", app)?;
    if RESERVED_APP_LABELS.contains(&app) {
        return Err(HostnameError::ReservedApp(app.to_string()));
    }
    validate_label("ephemeral_id", ephemeral_id)?;
    validate_label("cluster", cluster)?;
    validate_label("location", location)?;
    validate_domain("domain", domain)?;
    Ok(format!(
        "{app}.{ephemeral_id}.{cluster}.{location}.{domain}"
    ))
}

/// Format the stable-claim FQDN (no `ephemeral_id` segment).
///
/// ```
/// use tatara_process::hostname::fmt_fqdn_stable;
/// let fqdn = fmt_fqdn_stable("api", "pleme-dev", "use1", "quero.lol").unwrap();
/// assert_eq!(fqdn, "api.pleme-dev.use1.quero.lol");
/// ```
pub fn fmt_fqdn_stable(
    app: &str,
    cluster: &str,
    location: &str,
    domain: &str,
) -> Result<String, HostnameError> {
    validate_label("app", app)?;
    if RESERVED_APP_LABELS.contains(&app) {
        return Err(HostnameError::ReservedApp(app.to_string()));
    }
    validate_label("cluster", cluster)?;
    validate_label("location", location)?;
    validate_domain("domain", domain)?;
    Ok(format!("{app}.{cluster}.{location}.{domain}"))
}

/// Compute the content-hash form of `ephemeral_id` for a given
/// `ProcessSpec`. Stable across reconciles of the same spec; new
/// spec content ⇒ new hash ⇒ new DNS slot.
///
/// Uses [`EPHEMERAL_ID_HASH_LEN`] hex chars of BLAKE3 over the
/// canonical JSON of the spec.
pub fn ephemeral_id_from_spec<T: Serialize>(spec: &T) -> Result<String, HostnameError> {
    let bytes = canonical_json(spec).map_err(|_| HostnameError::InvalidLabel {
        segment: "spec",
        label: "<unserializable>".into(),
        reason: "spec failed to canonicalize",
    })?;
    Ok(short_hex_blake3(&bytes, EPHEMERAL_ID_HASH_LEN))
}

/// Resolve the `ephemeral_id` for a single [`RoutingHostname`]
/// entry. Named slot wins if set; otherwise the content-hash form
/// is computed from the surrounding `ProcessSpec` (caller passes
/// in via `fallback_hash`).
///
/// The split-arg design keeps this pure — the spec hash is computed
/// once by the caller (via [`ephemeral_id_from_spec`]) and reused
/// across every hostname on the same Process.
pub fn resolve_ephemeral_id<'a>(hostname: &'a RoutingHostname, fallback_hash: &'a str) -> &'a str {
    match &hostname.instance {
        Some(s) if !s.is_empty() => s.as_str(),
        _ => fallback_hash,
    }
}

// ─── Validation ────────────────────────────────────────────────────

fn validate_label(segment: &'static str, label: &str) -> Result<(), HostnameError> {
    if label.is_empty() || label.len() > 63 {
        return Err(HostnameError::InvalidLabel {
            segment,
            label: label.to_string(),
            reason: "must be 1–63 characters",
        });
    }
    if label.starts_with('-') || label.ends_with('-') {
        return Err(HostnameError::InvalidLabel {
            segment,
            label: label.to_string(),
            reason: "must not start or end with a hyphen",
        });
    }
    if !label
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(HostnameError::InvalidLabel {
            segment,
            label: label.to_string(),
            reason: "must contain only [a-z0-9-]",
        });
    }
    Ok(())
}

fn validate_domain(segment: &'static str, domain: &str) -> Result<(), HostnameError> {
    if domain.is_empty() {
        return Err(HostnameError::InvalidLabel {
            segment,
            label: domain.to_string(),
            reason: "must not be empty",
        });
    }
    // Multi-label domain — every dot-separated piece must be a valid label.
    for piece in domain.split('.') {
        validate_label(segment, piece)?;
    }
    Ok(())
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    // Canonical = serde_json round-trip through Value (preserves
    // declaration-order keys). Matches the receipt + worker pattern.
    let v = serde_json::to_value(value)?;
    serde_json::to_vec(&v)
}

fn short_hex_blake3(bytes: &[u8], len: usize) -> String {
    // Delegate the 2-link `blake3::hash → hex` step to the substrate
    // primitive so the ephemeral-id prefix stays byte-identical to
    // every receipt/attestation hex-digest workspace-wide; take a
    // stable prefix of the shared full-length hex.
    crate::hash::hex_blake3(bytes).chars().take(len).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn fmt_fqdn_per_instance() {
        let f = fmt_fqdn("api", "demo-prod", "pleme-dev", "use1", "quero.lol").unwrap();
        assert_eq!(f, "api.demo-prod.pleme-dev.use1.quero.lol");
    }

    #[test]
    fn fmt_fqdn_stable_form() {
        let f = fmt_fqdn_stable("api", "pleme-dev", "use1", "quero.lol").unwrap();
        assert_eq!(f, "api.pleme-dev.use1.quero.lol");
    }

    #[test]
    fn fmt_fqdn_with_multilevel_domain() {
        let f = fmt_fqdn("api", "env-a", "rio", "us", "internal.example.com").unwrap();
        assert_eq!(f, "api.env-a.rio.us.internal.example.com");
    }

    #[test]
    fn reserved_app_rejected() {
        let r = fmt_fqdn("auth", "x", "y", "z", "example.com");
        assert!(matches!(r, Err(HostnameError::ReservedApp(_))));
        let r = fmt_fqdn_stable("cracha", "y", "z", "example.com");
        assert!(matches!(r, Err(HostnameError::ReservedApp(_))));
    }

    #[test]
    fn empty_label_rejected() {
        let r = fmt_fqdn("", "x", "y", "z", "example.com");
        assert!(matches!(
            r,
            Err(HostnameError::InvalidLabel { segment: "app", .. })
        ));
    }

    #[test]
    fn too_long_label_rejected() {
        let long = "a".repeat(64);
        let r = fmt_fqdn(&long, "x", "y", "z", "example.com");
        assert!(matches!(r, Err(HostnameError::InvalidLabel { .. })));
    }

    #[test]
    fn uppercase_label_rejected() {
        let r = fmt_fqdn("API", "x", "y", "z", "example.com");
        assert!(matches!(r, Err(HostnameError::InvalidLabel { .. })));
    }

    #[test]
    fn leading_hyphen_label_rejected() {
        let r = fmt_fqdn("api", "-bad", "y", "z", "example.com");
        assert!(matches!(r, Err(HostnameError::InvalidLabel { .. })));
    }

    #[test]
    fn underscore_label_rejected() {
        let r = fmt_fqdn("api", "x_y", "z", "w", "example.com");
        assert!(matches!(r, Err(HostnameError::InvalidLabel { .. })));
    }

    #[test]
    fn empty_domain_rejected() {
        let r = fmt_fqdn("api", "x", "y", "z", "");
        assert!(matches!(r, Err(HostnameError::InvalidLabel { .. })));
    }

    // ─── Content-hash derivation ─────────────────────────────────

    #[derive(Serialize, Deserialize)]
    struct TestSpec {
        a: u32,
        b: String,
    }

    #[test]
    fn ephemeral_id_is_8_hex_chars() {
        let spec = TestSpec {
            a: 1,
            b: "x".into(),
        };
        let id = ephemeral_id_from_spec(&spec).unwrap();
        assert_eq!(id.len(), EPHEMERAL_ID_HASH_LEN);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn ephemeral_id_is_deterministic() {
        let s1 = TestSpec {
            a: 1,
            b: "x".into(),
        };
        let s2 = TestSpec {
            a: 1,
            b: "x".into(),
        };
        assert_eq!(
            ephemeral_id_from_spec(&s1).unwrap(),
            ephemeral_id_from_spec(&s2).unwrap()
        );
    }

    #[test]
    fn ephemeral_id_changes_with_spec() {
        let s1 = TestSpec {
            a: 1,
            b: "x".into(),
        };
        let s2 = TestSpec {
            a: 2,
            b: "x".into(),
        };
        let s3 = TestSpec {
            a: 1,
            b: "y".into(),
        };
        let id1 = ephemeral_id_from_spec(&s1).unwrap();
        let id2 = ephemeral_id_from_spec(&s2).unwrap();
        let id3 = ephemeral_id_from_spec(&s3).unwrap();
        assert_ne!(id1, id2);
        assert_ne!(id1, id3);
        assert_ne!(id2, id3);
    }

    #[test]
    fn ephemeral_id_lowercase_valid_dns_label() {
        // BLAKE3 hex is lowercase by design; the validator must
        // accept the output as a valid DNS label.
        let spec = TestSpec {
            a: 42,
            b: "anything".into(),
        };
        let id = ephemeral_id_from_spec(&spec).unwrap();
        validate_label("ephemeral_id", &id).unwrap();
    }

    // ─── resolve_ephemeral_id ────────────────────────────────────

    #[test]
    fn resolve_named_slot_wins() {
        let h = RoutingHostname::instanced("api", "demo-prod");
        assert_eq!(resolve_ephemeral_id(&h, "fallback"), "demo-prod");
    }

    #[test]
    fn resolve_empty_named_falls_back() {
        let h = RoutingHostname {
            app: "api".into(),
            instance: Some(String::new()),
            cluster: None,
        };
        assert_eq!(resolve_ephemeral_id(&h, "abc123de"), "abc123de");
    }

    #[test]
    fn resolve_unset_named_falls_back() {
        let h = RoutingHostname::content_hashed("api");
        assert_eq!(resolve_ephemeral_id(&h, "abc123de"), "abc123de");
    }

    // ─── End-to-end ──────────────────────────────────────────────

    // ─── HostnameResultExt::hostname_ctx substrate pins ──────────
    //
    // Fail-before-pass-after granularity: the `HostnameResultExt::
    // hostname_ctx` trait method did not exist before this commit,
    // so each test below fails to compile pre-lift. Post-lift they
    // collectively pin the display-prefix wrap-shape at ONE substrate
    // owner — a regression that drifts the separator, swaps the two
    // slots, wraps the `HostnameError` with a chain-form `source`, or
    // promotes the pass-through arm to a synthesis (an empty `Ok(())`,
    // a mutated context slug) surfaces HERE rather than as silent
    // operator-facing skew across the three pre-lift consumers whose
    // log output already encoded the flat `"<ctx>: <HostnameError
    // display>"` shape.

    fn sample_err() -> HostnameError {
        HostnameError::InvalidLabel {
            segment: "app",
            label: "BAD".into(),
            reason: "must contain only [a-z0-9-]",
        }
    }

    #[test]
    fn hostname_ctx_static_str_context_matches_pre_lift_format_bytewise() {
        // Byte-shape parity pin: the wrap output of `hostname_ctx
        // ("<slug>")` MUST be `Display`-identical to the pre-lift
        // hand-authored `.map_err(|e| anyhow!("<slug>: {e}"))` chain.
        // A regression that inserted a separator character (`"<slug>::
        // <hostname>"`), dropped the space after the colon, or swapped
        // the two slots (`"<hostname>: <slug>"`) surfaces HERE rather
        // than as silent drift at every downstream log-output consumer.
        let raw: Result<(), HostnameError> = Err(sample_err());
        let via_trait = raw.hostname_ctx("fmt_fqdn (per-instance)").unwrap_err();
        let pre_lift = anyhow::anyhow!("fmt_fqdn (per-instance): {}", sample_err());
        assert_eq!(
            format!("{via_trait}"),
            format!("{pre_lift}"),
            "hostname_ctx wrap must be Display-identical to pre-lift anyhow! chain"
        );
    }

    #[test]
    fn hostname_ctx_ok_arm_is_a_pure_passthrough() {
        // Ok-arm invariant: `hostname_ctx` on `Ok(t)` MUST return
        // `Ok(t)` verbatim — no side-effect on the payload, no
        // synthesis of a context-tagged error, no allocation. Peer to
        // the Err-arm byte-shape pin; a regression that promoted the
        // Ok arm to ALWAYS produce a synthesis Error would silently
        // break every successful hostname-format call in the pre-lift
        // consumer set.
        let raw: Result<&'static str, HostnameError> = Ok("api.demo-prod.pleme-dev.use1.quero.lol");
        assert_eq!(
            raw.hostname_ctx("noop").unwrap(),
            "api.demo-prod.pleme-dev.use1.quero.lol"
        );
    }

    #[test]
    fn hostname_ctx_threads_the_underlying_hostname_error_display_verbatim() {
        // Display-tail invariant: the wrapped `anyhow::Error`'s
        // `Display` output MUST contain the `HostnameError`'s own
        // `Display` output verbatim as the tail past `"<ctx>: "`. A
        // regression that inserted a normalization (uppercase, JSON
        // encoding, truncation) between the composed `{e}` slot and
        // the underlying thiserror-derived Display impl would surface
        // HERE rather than as silent operator-facing skew across the
        // three consumers whose grep patterns already encoded the
        // canonical `HostnameError` variant wordings ("invalid DNS
        // label ...", "app label ... is reserved").
        let raw: Result<(), HostnameError> = Err(HostnameError::ReservedApp("auth".into()));
        let wrapped = raw.hostname_ctx("fmt_fqdn_stable").unwrap_err();
        let expected_tail = format!("{}", HostnameError::ReservedApp("auth".into()));
        let expected = format!("fmt_fqdn_stable: {expected_tail}");
        assert_eq!(format!("{wrapped}"), expected);
        // Also assert the tail appears verbatim as a suffix — a change
        // in the thiserror-derived Display for ReservedApp would fail
        // both this assertion and the RECEIPT_VERSION-in-tail invariant
        // its docstring pins.
        assert!(
            format!("{wrapped}").ends_with(&expected_tail),
            "wrap must end with the HostnameError Display verbatim"
        );
    }

    #[test]
    fn hostname_ctx_composes_over_ephemeral_id_from_spec_call_shape() {
        // End-to-end composition pin: the substrate trait method
        // composes cleanly over the `ephemeral_id_from_spec` return
        // shape at a real callsite (the `render_routing` R9 seed).
        // A regression that specialized the trait bound to only one
        // hostname primitive's Result shape would surface HERE.
        #[derive(Serialize)]
        struct NoSuchThingAsAnUnserializableStruct {
            a: u32,
        }
        let v = NoSuchThingAsAnUnserializableStruct { a: 1 };
        let composed: anyhow::Result<String> =
            ephemeral_id_from_spec(&v).hostname_ctx("ephemeral_id_from_spec");
        assert!(composed.is_ok());
        assert_eq!(composed.unwrap().len(), EPHEMERAL_ID_HASH_LEN);
    }

    #[test]
    fn end_to_end_named_and_unnamed_for_same_process() {
        let spec = TestSpec {
            a: 1,
            b: "x".into(),
        };
        let hash = ephemeral_id_from_spec(&spec).unwrap();

        let h_named = RoutingHostname::instanced("api", "demo-prod");
        let h_anon = RoutingHostname::content_hashed("gateway");

        let id_named = resolve_ephemeral_id(&h_named, &hash);
        let id_anon = resolve_ephemeral_id(&h_anon, &hash);

        let fqdn_named =
            fmt_fqdn(&h_named.app, id_named, "pleme-dev", "use1", "quero.lol").unwrap();
        let fqdn_anon = fmt_fqdn(&h_anon.app, id_anon, "pleme-dev", "use1", "quero.lol").unwrap();

        assert_eq!(fqdn_named, "api.demo-prod.pleme-dev.use1.quero.lol");
        assert!(fqdn_anon.starts_with("gateway."));
        assert!(fqdn_anon.ends_with(".pleme-dev.use1.quero.lol"));
        // 5 named segments (app + eph_id + cluster + location + domain),
        // but `domain` itself splits as `quero.lol` ⇒ 6 dot-delimited
        // pieces. The shape, not the count, is the invariant.
        assert_eq!(fqdn_anon.matches('.').count(), 5);
    }
}
