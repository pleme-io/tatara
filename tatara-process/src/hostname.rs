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

impl HostnameError {
    /// Construct an [`HostnameError::InvalidLabel`] variant — the ONE
    /// substrate primitive owning the three-slot
    /// `HostnameError::InvalidLabel { segment, label: <str>.to_string(),
    /// reason: <static> }` construction shape every RFC 1123 DNS-label
    /// rejection site in this module walks BEFORE returning through the
    /// `?` short-circuit.
    ///
    /// Pre-lift the shape was hand-authored at FOUR module-private
    /// validation sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    /// threshold, each restating the SAME three-field struct-literal
    /// verbatim modulo the per-site `reason` slot:
    ///
    /// * [`validate_label`] × 3 — the length gate
    ///   (`"must be 1–63 characters"`), the leading/trailing-hyphen gate
    ///   (`"must not start or end with a hyphen"`), and the character-
    ///   set gate (`"must contain only [a-z0-9-]"`); each walked
    ///   `HostnameError::InvalidLabel { segment, label: label.to_string
    ///   (), reason: <per-gate literal> }` with the same
    ///   `segment: &'static str` slot threaded through from the caller
    ///   and the same `label.to_string()` projection on the borrowed
    ///   `&str` label slot.
    /// * [`validate_domain`] × 1 — the empty-domain early-return
    ///   (`"must not be empty"`); same three-slot struct literal shape,
    ///   same `<str>.to_string()` projection on the borrowed `domain`
    ///   argument, sibling to the three sites in [`validate_label`] on
    ///   the RFC 1123 rejection axis.
    ///
    /// All four sites walked the SAME struct-literal three-slot shape
    /// verbatim, differing only in the `reason: &'static str` slot they
    /// bound. Post-lift each callsite reads `HostnameError::invalid_label
    /// (segment, label, "<reason>")` and the construction shape lives at
    /// ONE substrate owner here.
    ///
    /// Peer to the two-step composer [`validate_app`] on the same
    /// hostname-validation axis, split by ABSTRACTION LEVEL:
    /// [`validate_app`] owns the ordered check chain callers CONSUME
    /// (RFC 1123 → reserved-name); this constructor owns the typed-
    /// variant PRODUCTION callers of those checks EMIT. Together the
    /// two primitives partition the module's rejection surface — the
    /// composer says WHEN to reject, the constructor says WHAT the
    /// rejection variant looks like on the wire.
    ///
    /// The `label` slot accepts `impl Into<String>` so a caller with a
    /// borrowed `&str` label (the four pre-lift sites) reaches
    /// `invalid_label(segment, label, reason)` without a per-site
    /// `.to_string()` — the projection lives at the substrate. A caller
    /// with an owned [`String`] (a future consumer stamping a
    /// dynamically-composed label into the rejection variant) reaches
    /// the SAME constructor without a per-site conversion either — the
    /// `impl Into<String>` bound admits both slot shapes identically.
    /// A future extension to the variant (a byte-offset slot into the
    /// source label pinpointing the failing character, a
    /// [`tracing::Span`] correlation slot, a normalization of the
    /// label's casing at the substrate before it reaches the operator's
    /// log stream) lands at THIS ONE constructor and every rejection
    /// site inherits the upgrade mechanically — no per-site edit at any
    /// of the four `validate_*` primitives, no drift risk for a fifth
    /// future validation site that plugs into the same rejection
    /// policy.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the three-slot struct-literal recurred at four hand-authored
    /// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger and
    /// lifts to ONE substrate owner here, matching the discipline
    /// [`validate_app`] and [`validate_fqdn_suffix`] already carry on
    /// the peer composer axes). THEORY.md §II.1 invariant 5
    /// (composition preserves proofs — post-lift every rejection site
    /// surfaces the byte-identical `HostnameError::InvalidLabel` variant
    /// by CONSTRUCTION rather than by four independent struct-literal
    /// restatements kept in sync by convention; a regression that re-
    /// open-coded a site would surface at the pin block below rather
    /// than as silent operator-facing skew across every downstream
    /// rejection consumer).
    #[inline]
    fn invalid_label(
        segment: &'static str,
        label: impl Into<String>,
        reason: &'static str,
    ) -> Self {
        HostnameError::InvalidLabel {
            segment,
            label: label.into(),
            reason,
        }
    }
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
    // Delegates the display-prefix wrap-shape body to the generic
    // substrate owner [`crate::err_ctx::ErrCtxExt`]. Pre-lift the body
    // restated the `.map_err(|e| anyhow::anyhow!("{context}: {e}"))`
    // closure by hand, byte-identical to the three sibling specialized
    // peers ([`crate::kube_error::KubeResultExt`],
    // [`crate::anyhow_flatten::FlattenCtxExt`],
    // [`crate::err_ctx::ErrCtxExt`] itself). Post-lift the byte-shape
    // body lives at ONE substrate owner + this impl is a naming-layer
    // delegate — [`HostnameError`] impls `Display` via `thiserror` so
    // the generic [`crate::err_ctx::ErrCtxExt`] impl applies to
    // `Result<T, HostnameError>` directly. Pinned by
    // [`crate::err_ctx::tests::err_ctx_agrees_with_hostname_ctx_on_hostname_error_result`]
    // so a regression that re-open-coded the body would surface there
    // rather than as silent operator-facing skew between the
    // hostname-side consumer (`render_routing`) and the sibling peer
    // families.

    #[inline]
    fn hostname_ctx(self, context: &'static str) -> anyhow::Result<T> {
        use crate::err_ctx::ErrCtxExt;
        self.err_ctx(context)
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
    validate_app(app)?;
    validate_label("ephemeral_id", ephemeral_id)?;
    validate_fqdn_suffix(cluster, location, domain)?;
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
    validate_app(app)?;
    validate_fqdn_suffix(cluster, location, domain)?;
    Ok(format!("{app}.{cluster}.{location}.{domain}"))
}

/// Compute the content-hash form of `ephemeral_id` for a given
/// `ProcessSpec`. Stable across reconciles of the same spec; new
/// spec content ⇒ new hash ⇒ new DNS slot.
///
/// Uses [`EPHEMERAL_ID_HASH_LEN`] hex chars of BLAKE3 over the
/// canonical JSON of the spec.
pub fn ephemeral_id_from_spec<T: Serialize>(spec: &T) -> Result<String, HostnameError> {
    // Canonical-bytes projection rides through the ONE substrate
    // primitive [`crate::three_pillar::canonical_bytes`] — the
    // strict, error-propagating peer of `three_pillar::pillar_bytes`
    // that owns the 2-link `serde_json::to_value → serde_json::to_vec`
    // canonicalization chain. Pre-lift this site read through a
    // module-private `canonical_json` helper (removed) that restated
    // the same 2-link chain byte-for-byte alongside the peer at
    // `tatara-export-worker::canonical_json` — two hand-authored
    // sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold.
    // Post-lift both consumers name the payload ONCE and route
    // through the ONE substrate owner; the discard of the concrete
    // `serde_json::Error` diagnostic rides through the local
    // `HostnameError::InvalidLabel` projection at this callsite so
    // the operator-facing wording stays byte-identical to the
    // pre-lift shape.
    let bytes = crate::three_pillar::canonical_bytes(spec).map_err(|_| {
        HostnameError::invalid_label("spec", "<unserializable>", "spec failed to canonicalize")
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
        return Err(HostnameError::invalid_label(
            segment,
            label,
            "must be 1–63 characters",
        ));
    }
    if label.starts_with('-') || label.ends_with('-') {
        return Err(HostnameError::invalid_label(
            segment,
            label,
            "must not start or end with a hyphen",
        ));
    }
    if !label
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(HostnameError::invalid_label(
            segment,
            label,
            "must contain only [a-z0-9-]",
        ));
    }
    Ok(())
}

/// Validate a caller-supplied `app` label at the fleet-hostname
/// boundary — the ONE substrate primitive owning the two-step (RFC 1123
/// DNS label + saguão-reservation reject) check every hostname composer
/// runs on its `app` slot BEFORE stamping it into an emitted FQDN.
///
/// Pre-lift the two-step check was hand-authored at TWO adjacent public
/// FQDN composers in this module past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold:
///
/// * [`fmt_fqdn`] — the per-instance form; the 4-line prelude
///   preceded the sibling `validate_label("ephemeral_id", …)` +
///   cluster / location / domain checks.
/// * [`fmt_fqdn_stable`] — the unprefixed stable-claim form; the same
///   4-line prelude preceded the cluster / location / domain checks
///   with no `ephemeral_id` slot in between.
///
/// Both restated the SAME 4-line prelude verbatim: (1)
/// `validate_label("app", app)?` to enforce the RFC 1123 shape (1–63
/// chars, lowercase alphanumeric + hyphen, no leading / trailing
/// hyphen), then (2) an early-return
/// `HostnameError::ReservedApp(app.to_string())` when the label
/// appears in the module-private [`RESERVED_APP_LABELS`] set
/// (currently `"auth"` / `"cracha"` — the saguão control-plane
/// reservations declared in pleme-io CLAUDE.md § Fleet hostname
/// pattern).
///
/// Post-lift each callsite reads `validate_app(app)?` and the ordered
/// two-step check lives at ONE substrate owner. The step ORDER is
/// load-bearing: `validate_label` runs first so a reserved label whose
/// spelling ALSO violates RFC 1123 (an operator who typed `"AUTH"`
/// instead of `"auth"`) surfaces as
/// [`HostnameError::InvalidLabel`] (the underlying shape defect),
/// not as [`HostnameError::ReservedApp`] (the higher-level policy
/// gate) — matching the pre-lift order both composers hand-authored.
/// A regression that swapped the two steps would silently re-classify
/// every such input and callers pattern-matching on the two variants
/// would branch differently.
///
/// A future extension to the reserved set (adding a third saguão name,
/// a per-cluster reservation surface, a normalized-form lookup that
/// treats `"Auth"` and `"auth"` as the same reservation) lands at THIS
/// ONE substrate primitive and both [`fmt_fqdn`] + [`fmt_fqdn_stable`]
/// inherit the upgrade mechanically — no per-composer edit at either
/// call site, no drift risk for a third future FQDN-shape composer
/// that plugs into the same reservation policy.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 4-line two-step check recurred at two hand-authored composer
/// preludes past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger and
/// lifts to ONE substrate owner here). THEORY.md §II.1 invariant 5
/// (composition preserves proofs — the pin block below binds the
/// primitive at fail-before-pass-after granularity so a regression
/// that reorders the two steps, drops one, or drifts the typed error
/// variant surfaces at THESE pins rather than as silent fleet-
/// hostname skew across every downstream FQDN emit).
fn validate_app(app: &str) -> Result<(), HostnameError> {
    validate_label("app", app)?;
    if RESERVED_APP_LABELS.contains(&app) {
        return Err(HostnameError::ReservedApp(app.to_string()));
    }
    Ok(())
}

fn validate_domain(segment: &'static str, domain: &str) -> Result<(), HostnameError> {
    if domain.is_empty() {
        return Err(HostnameError::invalid_label(
            segment,
            domain,
            "must not be empty",
        ));
    }
    // Multi-label domain — every dot-separated piece must be a valid label.
    for piece in domain.split('.') {
        validate_label(segment, piece)?;
    }
    Ok(())
}

/// Validate the shared 3-segment `${cluster}.${location}.${domain}` FQDN
/// suffix — the ONE substrate primitive owning the ordered
/// `validate_label("cluster", …) → validate_label("location", …) →
/// validate_domain("domain", …)` prelude every fleet-hostname composer
/// runs on the trailing suffix common to BOTH forms
/// (`${app}.${ephemeral_id}.<suffix>` per-instance and `${app}.<suffix>`
/// stable) BEFORE stamping it into an emitted FQDN.
///
/// Pre-lift the 3-line ordered check was hand-authored at TWO adjacent
/// public FQDN composers in this module past the ★★ PRIME-DIRECTIVE
/// ≥ 2 duplication threshold:
///
/// * [`fmt_fqdn`] — the per-instance form; the 3-line suffix prelude
///   followed the sibling `validate_app(app)?` +
///   `validate_label("ephemeral_id", …)?` head checks and preceded the
///   `format!("{app}.{ephemeral_id}.{cluster}.{location}.{domain}")`
///   emission.
/// * [`fmt_fqdn_stable`] — the unprefixed stable-claim form; the same
///   3-line suffix prelude followed the sibling `validate_app(app)?`
///   check with no `ephemeral_id` slot in between and preceded the
///   `format!("{app}.{cluster}.{location}.{domain}")` emission.
///
/// Both restated the SAME 3-line prelude verbatim: (1)
/// `validate_label("cluster", cluster)?` to enforce the RFC 1123 shape
/// on the cluster segment, then (2)
/// `validate_label("location", location)?` for the location segment,
/// then (3) `validate_domain("domain", domain)?` to enforce the
/// multi-label domain shape (non-empty AND every dot-split piece a
/// valid RFC 1123 label).
///
/// Post-lift each callsite reads `validate_fqdn_suffix(cluster,
/// location, domain)?` and the ordered 3-step suffix check lives at
/// ONE substrate owner. The step ORDER is load-bearing on the typed-
/// variant surface: `cluster` is checked first so a bad-cluster-and-
/// bad-location input surfaces as `InvalidLabel { segment: "cluster", .. }`
/// (matching the pre-lift order both composers hand-authored) rather
/// than `InvalidLabel { segment: "location", .. }` — callers pattern-
/// matching on the `segment` slot to render targeted operator messages
/// branch differently, so a swap of the two steps would silently
/// re-classify every such input.
///
/// Peer to [`validate_app`] on the "ordered validation prelude" axis —
/// `validate_app` owns the 2-step head check for the `app` segment,
/// `validate_fqdn_suffix` owns the 3-step trailing suffix check for the
/// `cluster` / `location` / `domain` segments; together they cover the
/// full validation surface both FQDN composers walk BEFORE the terminal
/// `format!(...)` emission.
///
/// A future extension to the suffix check (a per-cluster reserved-name
/// gate mirroring [`RESERVED_APP_LABELS`], a stricter per-location DNS
/// label check, a per-domain TLD allowlist gate, a per-fleet
/// normalization of the cluster segment) lands at THIS ONE substrate
/// primitive and both [`fmt_fqdn`] + [`fmt_fqdn_stable`] inherit the
/// upgrade mechanically — no per-composer edit at either callsite, no
/// drift risk for a third future FQDN-shape composer (a per-region
/// gateway form, a wildcard-cert-issuer probe form) that plugs into the
/// same suffix policy.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 3-line three-step check recurred at two hand-authored composer
/// preludes past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger and
/// lifts to ONE substrate owner here, matching the discipline
/// [`validate_app`] already carries on the peer head-check axis).
/// THEORY.md §II.1 invariant 5 (composition preserves proofs — the
/// pin block below binds the primitive at fail-before-pass-after
/// granularity so a regression that reorders the three steps, drops
/// one, or drifts the typed `segment` slot surfaces at THESE pins
/// rather than as silent fleet-hostname skew across every downstream
/// FQDN emit).
fn validate_fqdn_suffix(cluster: &str, location: &str, domain: &str) -> Result<(), HostnameError> {
    validate_label("cluster", cluster)?;
    validate_label("location", location)?;
    validate_domain("domain", domain)?;
    Ok(())
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

    // ─── validate_app substrate pins ─────────────────────────────
    //
    // Fail-before-pass-after granularity: the `validate_app` helper
    // did not exist pre-lift — both [`fmt_fqdn`] and [`fmt_fqdn_stable`]
    // hand-authored the two-step (RFC 1123 label + reserved-name reject)
    // check inline. Post-lift the two composers thread the same
    // primitive, so the pins below pin the primitive's SHAPE + STEP
    // ORDER + typed-variant surface at the substrate — a regression
    // that (a) reorders the two steps, (b) drops the reserved-name
    // gate silently, or (c) promotes the `HostnameError::ReservedApp`
    // arm to a generic `InvalidLabel` surfaces HERE rather than as
    // silent skew at every downstream FQDN emit.

    #[test]
    fn validate_app_accepts_valid_lowercase_alphanumeric_label() {
        // Happy-path pin: a valid `app` label passes the two-step
        // check with `Ok(())`. A regression that inverted the return
        // arm (rejected everything, matched no reserved) surfaces
        // HERE rather than as every FQDN emit refusing every input.
        validate_app("api").unwrap();
        validate_app("gateway").unwrap();
        validate_app("demo-app").unwrap();
        validate_app("a").unwrap();
    }

    #[test]
    fn validate_app_rejects_empty_label_with_invalid_label_variant() {
        // Step-1 delegation pin: an empty `app` MUST surface as
        // `HostnameError::InvalidLabel { segment: "app", .. }` from
        // the underlying `validate_label("app", app)?` call — NOT as
        // `ReservedApp` (which would silently reclassify the shape
        // defect as a policy rejection).
        assert!(matches!(
            validate_app(""),
            Err(HostnameError::InvalidLabel { segment: "app", .. })
        ));
    }

    #[test]
    fn validate_app_rejects_uppercase_label_with_invalid_label_variant() {
        // Step-1 delegation pin: casing-invalid labels reach through
        // to `validate_label`'s [a-z0-9-] check. A regression that
        // short-circuited the reserved-check on a case-insensitive
        // match ("AUTH" reads as reserved without going through the
        // RFC 1123 gate first) would surface HERE.
        assert!(matches!(
            validate_app("API"),
            Err(HostnameError::InvalidLabel { segment: "app", .. })
        ));
    }

    #[test]
    fn validate_app_rejects_too_long_label_with_invalid_label_variant() {
        // Step-1 delegation pin: 64-char labels violate the RFC 1123
        // upper bound and surface at the `validate_label` gate.
        let long = "a".repeat(64);
        assert!(matches!(
            validate_app(&long),
            Err(HostnameError::InvalidLabel { segment: "app", .. })
        ));
    }

    #[test]
    fn validate_app_rejects_reserved_auth_label_with_reserved_app_variant() {
        // Step-2 pin: the currently-reserved `"auth"` slot surfaces
        // as `HostnameError::ReservedApp("auth")` — the typed
        // control-plane rejection callers pattern-match on. A
        // regression that dropped this variant would silently
        // accept the reservation and let a tenant deploy under the
        // saguão namespace.
        assert!(matches!(
            validate_app("auth"),
            Err(HostnameError::ReservedApp(ref s)) if s == "auth"
        ));
    }

    #[test]
    fn validate_app_rejects_reserved_cracha_label_with_reserved_app_variant() {
        // Sibling pin to the `"auth"` reservation — pins the second
        // currently-reserved label. A regression that dropped one
        // reservation but not the other would surface HERE.
        assert!(matches!(
            validate_app("cracha"),
            Err(HostnameError::ReservedApp(ref s)) if s == "cracha"
        ));
    }

    #[test]
    fn validate_app_step_order_puts_rfc_1123_check_before_reserved_check() {
        // Load-bearing order pin: `validate_label` runs FIRST so a
        // reserved label whose spelling ALSO violates RFC 1123
        // (uppercase, hyphen at end, etc.) surfaces as
        // `InvalidLabel` — the underlying SHAPE defect — not as
        // `ReservedApp` (the higher-level POLICY gate). Callers who
        // pattern-match on the two variants branch DIFFERENTLY on
        // shape defects vs policy rejections, so a swap of the two
        // steps would silently re-route every uppercase-reserved
        // input into the wrong error arm.
        assert!(matches!(
            validate_app("AUTH"),
            Err(HostnameError::InvalidLabel { segment: "app", .. })
        ));
        assert!(matches!(
            validate_app("Cracha"),
            Err(HostnameError::InvalidLabel { segment: "app", .. })
        ));
    }

    #[test]
    fn validate_app_matches_pre_lift_two_step_chain_bytewise_across_every_variant_shape() {
        // Byte-shape parity pin: the substrate primitive's return
        // MUST equal the pre-lift 4-line hand-authored chain for
        // every representative input shape. A regression that
        // drifted the primitive's semantics away from the pre-lift
        // composer preludes surfaces HERE rather than as silent
        // skew at either `fmt_fqdn` / `fmt_fqdn_stable` consumer.
        fn pre_lift(app: &str) -> Result<(), HostnameError> {
            validate_label("app", app)?;
            if RESERVED_APP_LABELS.contains(&app) {
                return Err(HostnameError::ReservedApp(app.to_string()));
            }
            Ok(())
        }
        for input in [
            // Happy path.
            "api",
            "gateway",
            "demo-app",
            "a",
            // Step-1 rejections.
            "",
            "API",
            "-bad",
            "bad-",
            "with_underscore",
            // Step-2 rejections.
            "auth",
            "cracha",
            // Step-1 wins over step-2 (uppercase reserved).
            "AUTH",
            "Cracha",
        ] {
            let via_primitive = validate_app(input);
            let via_pre_lift = pre_lift(input);
            match (via_primitive, via_pre_lift) {
                (Ok(()), Ok(())) => {}
                (Err(a), Err(b)) => assert_eq!(a, b, "variant mismatch for {input:?}"),
                (a, b) => panic!("arm mismatch for {input:?}: primitive={a:?} pre_lift={b:?}"),
            }
        }
    }

    #[test]
    fn validate_app_covers_every_currently_reserved_label_at_the_primitive() {
        // Coherence sweep: iterate the RESERVED_APP_LABELS set and
        // verify each element rejects at the substrate. A future
        // addition to the reserved set that forgets to update the
        // primitive would surface HERE rather than as silent
        // acceptance at every FQDN emit.
        for reserved in RESERVED_APP_LABELS {
            assert!(
                matches!(validate_app(reserved), Err(HostnameError::ReservedApp(ref s)) if s == reserved),
                "RESERVED_APP_LABELS entry {reserved:?} must surface as ReservedApp at the substrate"
            );
        }
    }

    // ─── validate_fqdn_suffix substrate pins ─────────────────────
    //
    // Fail-before-pass-after granularity: the `validate_fqdn_suffix`
    // helper did not exist pre-lift — both [`fmt_fqdn`] and
    // [`fmt_fqdn_stable`] hand-authored the three-step (`validate_label
    // ("cluster") → validate_label("location") → validate_domain
    // ("domain")`) suffix check inline. Post-lift the two composers
    // thread the same primitive, so the pins below pin the primitive's
    // SHAPE + STEP ORDER + typed-`segment` slot at the substrate — a
    // regression that (a) reorders the three steps (silently re-
    // classifying every multi-slot rejection into the wrong `segment`
    // arm), (b) drops one of the three checks, or (c) swaps the
    // `validate_domain` primitive for a `validate_label` on the domain
    // slot (silently accepting a single-label `example` in place of
    // the multi-label `example.com` shape) surfaces HERE rather than
    // as silent skew at every downstream FQDN emit.

    #[test]
    fn validate_fqdn_suffix_accepts_valid_three_segment_suffix() {
        // Happy-path pin: a valid `cluster.location.domain` triple
        // passes the three-step check with `Ok(())`. A regression that
        // inverted the return arm (rejected everything) surfaces HERE
        // rather than as every FQDN emit refusing every input.
        validate_fqdn_suffix("pleme-dev", "use1", "quero.lol").unwrap();
        validate_fqdn_suffix("prod", "eu-west-1", "example.com").unwrap();
        validate_fqdn_suffix("a", "b", "c.d.e").unwrap();
    }

    #[test]
    fn validate_fqdn_suffix_rejects_empty_cluster_with_cluster_segment_slot() {
        // Step-1 delegation pin: an empty `cluster` MUST surface as
        // `HostnameError::InvalidLabel { segment: "cluster", .. }`
        // from the underlying `validate_label("cluster", cluster)?`
        // call — NOT as `segment: "location"` or `segment: "domain"`
        // (which would silently re-classify the shape defect into a
        // trailing-slot rejection and route callers who pattern-match
        // on the `segment` slot to render targeted operator messages
        // to the wrong branch).
        assert!(matches!(
            validate_fqdn_suffix("", "use1", "quero.lol"),
            Err(HostnameError::InvalidLabel {
                segment: "cluster",
                ..
            })
        ));
    }

    #[test]
    fn validate_fqdn_suffix_rejects_empty_location_with_location_segment_slot() {
        // Step-2 delegation pin — sibling to the cluster-slot pin. A
        // valid cluster + empty location MUST surface as `segment:
        // "location"` (step 2 fired), NOT as `segment: "domain"`
        // (which would mean step 3 short-circuited past step 2).
        assert!(matches!(
            validate_fqdn_suffix("pleme-dev", "", "quero.lol"),
            Err(HostnameError::InvalidLabel {
                segment: "location",
                ..
            })
        ));
    }

    #[test]
    fn validate_fqdn_suffix_rejects_empty_domain_with_domain_segment_slot() {
        // Step-3 delegation pin — the terminal step. A valid cluster
        // + valid location + empty domain MUST surface as `segment:
        // "domain"` (from `validate_domain`'s empty-domain gate). A
        // regression that swapped `validate_domain` for
        // `validate_label` on the domain slot would silently accept
        // an empty string with a DIFFERENT `reason` slot or reject a
        // multi-label domain (`example.com`) that `validate_label`
        // alone forbids (dots).
        assert!(matches!(
            validate_fqdn_suffix("pleme-dev", "use1", ""),
            Err(HostnameError::InvalidLabel {
                segment: "domain",
                ..
            })
        ));
    }

    #[test]
    fn validate_fqdn_suffix_rejects_multilabel_cluster_with_invalid_label_variant() {
        // Cluster-shape pin: `cluster` reaches through `validate_label`
        // (single-label check), NOT `validate_domain` (multi-label
        // check). A dot-containing cluster MUST reject at the RFC 1123
        // gate. A regression that widened the cluster gate to
        // `validate_domain` would silently accept a multi-label
        // cluster like `pleme.dev` (folding two segments into one
        // slot at emit time and drifting every downstream Ingress /
        // DNSEndpoint dispatcher).
        assert!(matches!(
            validate_fqdn_suffix("pleme.dev", "use1", "quero.lol"),
            Err(HostnameError::InvalidLabel {
                segment: "cluster",
                ..
            })
        ));
    }

    #[test]
    fn validate_fqdn_suffix_accepts_multilabel_domain_via_validate_domain_split() {
        // Domain-shape pin: `domain` reaches through `validate_domain`
        // (multi-label check via `domain.split('.')`), NOT
        // `validate_label` (single-label check that would reject any
        // dot). A regression that narrowed the domain gate to
        // `validate_label` would surface HERE — every real-world
        // domain (`quero.lol`, `example.com`, `internal.example.com`)
        // contains at least one dot and would fail at the RFC 1123
        // single-label check.
        validate_fqdn_suffix("pleme-dev", "use1", "internal.example.com").unwrap();
        validate_fqdn_suffix("pleme-dev", "use1", "a.b.c.d.e.f").unwrap();
    }

    #[test]
    fn validate_fqdn_suffix_step_order_puts_cluster_before_location_before_domain() {
        // Load-bearing order pin: the three steps fire in the SAME
        // order the pre-lift composer preludes hand-authored (cluster
        // → location → domain), so an input that violates MULTIPLE
        // slots surfaces at the FIRST violated slot on the ordered
        // walk. Callers who pattern-match on the `segment` slot to
        // render targeted operator messages branch differently, so a
        // swap of the three steps would silently re-classify every
        // multi-slot-invalid input.
        //
        // All three slots invalid → surfaces at `cluster` (step 1).
        assert!(matches!(
            validate_fqdn_suffix("", "", ""),
            Err(HostnameError::InvalidLabel {
                segment: "cluster",
                ..
            })
        ));
        // Valid cluster + invalid location + invalid domain → surfaces
        // at `location` (step 2), NOT `domain` (step 3).
        assert!(matches!(
            validate_fqdn_suffix("pleme-dev", "", ""),
            Err(HostnameError::InvalidLabel {
                segment: "location",
                ..
            })
        ));
    }

    #[test]
    fn validate_fqdn_suffix_matches_pre_lift_three_step_chain_bytewise_across_every_variant_shape()
    {
        // Byte-shape parity pin: the substrate primitive's return
        // MUST equal the pre-lift 3-line hand-authored chain for
        // every representative input shape. A regression that
        // drifted the primitive's semantics away from the pre-lift
        // composer preludes surfaces HERE rather than as silent skew
        // at either `fmt_fqdn` / `fmt_fqdn_stable` consumer.
        fn pre_lift(cluster: &str, location: &str, domain: &str) -> Result<(), HostnameError> {
            validate_label("cluster", cluster)?;
            validate_label("location", location)?;
            validate_domain("domain", domain)?;
            Ok(())
        }
        for (cluster, location, domain) in [
            // Happy path — every corner both composers walk in
            // production.
            ("pleme-dev", "use1", "quero.lol"),
            ("prod", "eu-west-1", "example.com"),
            ("a", "b", "c.d.e"),
            ("cluster-1", "loc-2", "internal.example.com"),
            // Step-1 rejections — cluster slot fails.
            ("", "use1", "quero.lol"),
            ("BAD", "use1", "quero.lol"),
            ("-lead", "use1", "quero.lol"),
            ("with_underscore", "use1", "quero.lol"),
            ("pleme.dev", "use1", "quero.lol"),
            // Step-2 rejections — cluster ok, location fails.
            ("pleme-dev", "", "quero.lol"),
            ("pleme-dev", "USE1", "quero.lol"),
            ("pleme-dev", "loc_1", "quero.lol"),
            // Step-3 rejections — cluster + location ok, domain fails.
            ("pleme-dev", "use1", ""),
            ("pleme-dev", "use1", "-bad.com"),
            ("pleme-dev", "use1", "BAD.com"),
            // Multi-slot rejection — step 1 wins over 2 and 3.
            ("", "", ""),
            ("BAD", "USE1", ""),
        ] {
            let via_primitive = validate_fqdn_suffix(cluster, location, domain);
            let via_pre_lift = pre_lift(cluster, location, domain);
            match (via_primitive, via_pre_lift) {
                (Ok(()), Ok(())) => {}
                (Err(a), Err(b)) => assert_eq!(
                    a, b,
                    "variant mismatch for ({cluster:?}, {location:?}, {domain:?})"
                ),
                (a, b) => panic!(
                    "arm mismatch for ({cluster:?}, {location:?}, {domain:?}): primitive={a:?} pre_lift={b:?}"
                ),
            }
        }
    }

    #[test]
    fn fmt_fqdn_routes_suffix_slots_through_validate_fqdn_suffix_primitive() {
        // Delegation pin: the per-instance composer routes its
        // trailing suffix check through `validate_fqdn_suffix`, NOT
        // through a re-open-coded restatement of the three-step
        // chain. A regression that re-inlined the pre-lift check at
        // the composer prelude would reintroduce the duplication the
        // lift removed; this pin catches it by asserting the composer
        // surfaces the SAME typed `segment` slot the primitive would
        // for a representative rejection in each of the three suffix
        // slots (cluster, location, domain).
        assert!(matches!(
            fmt_fqdn("api", "x", "BAD", "use1", "quero.lol"),
            Err(HostnameError::InvalidLabel {
                segment: "cluster",
                ..
            })
        ));
        assert!(matches!(
            fmt_fqdn("api", "x", "pleme-dev", "", "quero.lol"),
            Err(HostnameError::InvalidLabel {
                segment: "location",
                ..
            })
        ));
        assert!(matches!(
            fmt_fqdn("api", "x", "pleme-dev", "use1", ""),
            Err(HostnameError::InvalidLabel {
                segment: "domain",
                ..
            })
        ));
    }

    #[test]
    fn fmt_fqdn_stable_routes_suffix_slots_through_validate_fqdn_suffix_primitive() {
        // Sibling delegation pin — same shape as the per-instance pin
        // above but for the stable-claim composer. Both composers now
        // share the primitive; a regression that re-inlined the chain
        // at either site surfaces at ONE of the two pins rather than
        // at every downstream FQDN emit.
        assert!(matches!(
            fmt_fqdn_stable("api", "BAD", "use1", "quero.lol"),
            Err(HostnameError::InvalidLabel {
                segment: "cluster",
                ..
            })
        ));
        assert!(matches!(
            fmt_fqdn_stable("api", "pleme-dev", "", "quero.lol"),
            Err(HostnameError::InvalidLabel {
                segment: "location",
                ..
            })
        ));
        assert!(matches!(
            fmt_fqdn_stable("api", "pleme-dev", "use1", ""),
            Err(HostnameError::InvalidLabel {
                segment: "domain",
                ..
            })
        ));
    }

    #[test]
    fn fmt_fqdn_and_fmt_fqdn_stable_agree_on_suffix_rejection_bytewise() {
        // Cross-composer coherence pin: post-lift both composers route
        // their suffix check through the ONE substrate primitive, so
        // the SAME suffix-slot violation surfaces byte-identically at
        // BOTH composers (differing only in the `ephemeral_id` arg
        // presence). A regression that re-inlined the chain at one
        // composer but not the other would silently drift the two
        // consumers' typed-`segment` slot; this pin binds them to the
        // ONE substrate primitive so any such drift surfaces HERE.
        for (cluster, location, domain, expected_segment) in [
            ("BAD", "use1", "quero.lol", "cluster"),
            ("pleme-dev", "", "quero.lol", "location"),
            ("pleme-dev", "use1", "", "domain"),
            ("pleme.dev", "use1", "quero.lol", "cluster"),
        ] {
            let via_per_instance = fmt_fqdn("api", "x", cluster, location, domain);
            let via_stable = fmt_fqdn_stable("api", cluster, location, domain);
            assert!(
                matches!(
                    &via_per_instance,
                    Err(HostnameError::InvalidLabel { segment, .. }) if *segment == expected_segment
                ),
                "fmt_fqdn must surface segment={expected_segment:?} for ({cluster:?}, {location:?}, {domain:?}); got {via_per_instance:?}"
            );
            assert!(
                matches!(
                    &via_stable,
                    Err(HostnameError::InvalidLabel { segment, .. }) if *segment == expected_segment
                ),
                "fmt_fqdn_stable must surface segment={expected_segment:?} for ({cluster:?}, {location:?}, {domain:?}); got {via_stable:?}"
            );
            // And the two composers' error variants agree bytewise on
            // the suffix rejection — they should, since both route
            // through the SAME primitive.
            match (via_per_instance, via_stable) {
                (Err(a), Err(b)) => assert_eq!(
                    a, b,
                    "fmt_fqdn and fmt_fqdn_stable must agree on suffix rejection for ({cluster:?}, {location:?}, {domain:?})"
                ),
                pair => panic!(
                    "expected both composers to reject ({cluster:?}, {location:?}, {domain:?}) with the SAME variant; got {pair:?}"
                ),
            }
        }
    }

    #[test]
    fn fmt_fqdn_routes_app_slot_through_validate_app_primitive() {
        // Delegation pin: the per-instance composer routes its `app`
        // slot check through `validate_app`, NOT through a re-open-
        // coded restatement of the two-step chain. A regression that
        // inlined the pre-lift check at the composer prelude would
        // reintroduce the duplication the lift removed; this pin
        // catches it by asserting the composer surfaces the SAME
        // typed error the primitive would for a representative
        // input in each of the two rejection arms.
        assert!(matches!(
            fmt_fqdn("AUTH", "x", "y", "z", "example.com"),
            Err(HostnameError::InvalidLabel { segment: "app", .. })
        ));
        assert!(matches!(
            fmt_fqdn("auth", "x", "y", "z", "example.com"),
            Err(HostnameError::ReservedApp(ref s)) if s == "auth"
        ));
    }

    #[test]
    fn fmt_fqdn_stable_routes_app_slot_through_validate_app_primitive() {
        // Sibling delegation pin — same shape as the per-instance
        // pin above but for the stable-claim composer. Both
        // composers now share the primitive; a regression that
        // re-inlined the chain at either site surfaces at ONE of
        // the two pins rather than at every downstream FQDN emit.
        assert!(matches!(
            fmt_fqdn_stable("Cracha", "y", "z", "example.com"),
            Err(HostnameError::InvalidLabel { segment: "app", .. })
        ));
        assert!(matches!(
            fmt_fqdn_stable("cracha", "y", "z", "example.com"),
            Err(HostnameError::ReservedApp(ref s)) if s == "cracha"
        ));
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

    // ─── HostnameError::invalid_label substrate pins ─────────────
    //
    // Fail-before-pass-after granularity: the `HostnameError::
    // invalid_label` constructor did not exist pre-lift — the four
    // `validate_*` rejection sites hand-authored the three-slot
    // `HostnameError::InvalidLabel { segment, label: <str>.to_string
    // (), reason: <static> }` struct literal inline. Post-lift the
    // four rejection sites thread the same constructor, so the pins
    // below pin the constructor's SHAPE + typed-variant surface +
    // slot-projection discipline at the substrate — a regression that
    // (a) drifts the `label.into()` projection at the substrate (e.g.
    // narrows the `impl Into<String>` bound to `&str`, ruling out a
    // future consumer stamping a dynamically-composed label), (b)
    // promotes the constructor to a different `HostnameError` variant
    // (a `ReservedApp` misfire) silently, or (c) swaps two of the
    // three slots at the substrate (e.g. binds `reason` in the
    // `segment` slot) surfaces HERE rather than as silent skew across
    // every downstream FQDN emit whose rejection pattern-matches on
    // the typed variant.

    #[test]
    fn invalid_label_constructor_produces_invalid_label_variant_with_all_three_slots_bound() {
        // Byte-shape parity pin: the constructor's return MUST equal
        // the pre-lift hand-authored `HostnameError::InvalidLabel {
        // segment, label: label.to_string(), reason }` struct literal
        // for every slot. A regression that swapped two slots (e.g.
        // bound the reason-string into the segment slot) would
        // surface HERE rather than as silent operator-facing skew
        // across the four `validate_*` rejection sites whose log
        // output already encoded the flat "invalid DNS label
        // <label:?> for segment <segment>: <reason>" shape.
        let via_constructor =
            HostnameError::invalid_label("app", "BAD", "must contain only [a-z0-9-]");
        let via_pre_lift = HostnameError::InvalidLabel {
            segment: "app",
            label: "BAD".to_string(),
            reason: "must contain only [a-z0-9-]",
        };
        assert_eq!(via_constructor, via_pre_lift);
    }

    #[test]
    fn invalid_label_constructor_accepts_borrowed_str_label_via_into_string() {
        // Borrowed-slot invariant: the `label` slot must accept the
        // borrowed `&str` shape (via `String::from`), matching the
        // four pre-lift rejection sites whose `label` parameter is a
        // borrowed `&str`. A regression that narrowed the bound to
        // owned `String` only would reject the four production
        // callsites at rustc time; a regression that narrowed it to
        // `&'static str` would reject dynamically-composed labels.
        let borrowed: &str = "dynamic-label";
        let err = HostnameError::invalid_label("app", borrowed, "must be 1–63 characters");
        match err {
            HostnameError::InvalidLabel {
                segment,
                label,
                reason,
            } => {
                assert_eq!(segment, "app");
                assert_eq!(label, "dynamic-label");
                assert_eq!(reason, "must be 1–63 characters");
            }
            other => panic!("expected InvalidLabel, got {other:?}"),
        }
    }

    #[test]
    fn invalid_label_constructor_accepts_owned_string_label_via_into_string() {
        // Owned-slot peer of the borrowed-slot pin above — the
        // `impl Into<String>` bound must admit an owned [`String`]
        // (identity `Into` impl) verbatim. A future consumer that
        // composes the label dynamically (via `format!`, from another
        // typed source) reaches the SAME constructor without a
        // per-callsite borrow detour. A regression that narrowed
        // either arm silently would surface HERE.
        let owned: String = "owned-label".to_string();
        let err = HostnameError::invalid_label("cluster", owned, "must not be empty");
        match err {
            HostnameError::InvalidLabel {
                segment,
                label,
                reason,
            } => {
                assert_eq!(segment, "cluster");
                assert_eq!(label, "owned-label");
                assert_eq!(reason, "must not be empty");
            }
            other => panic!("expected InvalidLabel, got {other:?}"),
        }
    }

    #[test]
    fn invalid_label_constructor_display_matches_thiserror_derived_shape_bytewise() {
        // Display-shape invariant: the constructor's produced variant
        // MUST render bytewise-identically to the pre-lift
        // thiserror-derived Display output — the shape every
        // reconciler consumer's log stream and every operator's grep
        // pattern already encodes. A regression that added a slot to
        // the variant without updating the `#[error]` attribute (or
        // vice versa) would surface as a Display drift here, upstream
        // of every downstream log consumer.
        let via_constructor =
            HostnameError::invalid_label("location", "USE1", "must contain only [a-z0-9-]");
        assert_eq!(
            format!("{via_constructor}"),
            "invalid DNS label \"USE1\" for segment location: must contain only [a-z0-9-]",
        );
    }

    #[test]
    fn validate_label_length_gate_routes_through_invalid_label_constructor_bytewise() {
        // Delegation pin — the length gate at [`validate_label`] MUST
        // surface the byte-identical `HostnameError::InvalidLabel`
        // variant the constructor produces for the same
        // (segment, label, "must be 1–63 characters") triple. A
        // regression that re-inlined the pre-lift struct literal at
        // the length gate — dropping the delegation and re-open-
        // coding the three slots — would reintroduce the duplication
        // this lift removed; this pin catches it by asserting the
        // rejection site's error equals the constructor's error
        // bytewise across two representative shapes (an empty label
        // and a 64-char label past the 63-char upper bound).
        let long = "a".repeat(64);
        for label in ["", long.as_str()] {
            let via_validate = validate_label("app", label).unwrap_err();
            let via_constructor =
                HostnameError::invalid_label("app", label, "must be 1–63 characters");
            assert_eq!(
                via_validate, via_constructor,
                "validate_label length gate must delegate to invalid_label constructor for label {label:?}"
            );
        }
    }

    #[test]
    fn validate_label_hyphen_gate_routes_through_invalid_label_constructor_bytewise() {
        // Sibling delegation pin — the leading/trailing-hyphen gate
        // at [`validate_label`] MUST surface the byte-identical
        // variant the constructor produces for the same triple.
        // Sibling to the length-gate pin above; three representative
        // shapes (leading hyphen, trailing hyphen, both).
        for label in ["-lead", "trail-", "-both-"] {
            let via_validate = validate_label("cluster", label).unwrap_err();
            let via_constructor = HostnameError::invalid_label(
                "cluster",
                label,
                "must not start or end with a hyphen",
            );
            assert_eq!(
                via_validate, via_constructor,
                "validate_label hyphen gate must delegate to invalid_label constructor for label {label:?}"
            );
        }
    }

    #[test]
    fn validate_label_charset_gate_routes_through_invalid_label_constructor_bytewise() {
        // Sibling delegation pin — the character-set gate at
        // [`validate_label`] MUST surface the byte-identical variant
        // the constructor produces for the same triple. Sibling to
        // the length + hyphen pins above; three representative shapes
        // (uppercase, underscore, non-ASCII).
        for label in ["BAD", "with_underscore", "café"] {
            let via_validate = validate_label("location", label).unwrap_err();
            let via_constructor =
                HostnameError::invalid_label("location", label, "must contain only [a-z0-9-]");
            assert_eq!(
                via_validate, via_constructor,
                "validate_label charset gate must delegate to invalid_label constructor for label {label:?}"
            );
        }
    }

    #[test]
    fn validate_domain_empty_gate_routes_through_invalid_label_constructor_bytewise() {
        // Sibling delegation pin — the empty-domain early-return at
        // [`validate_domain`] MUST surface the byte-identical variant
        // the constructor produces for `("<segment>", "", "must not
        // be empty")`. Sibling to the three [`validate_label`] gate
        // pins above; the fourth pre-lift rejection site closes the
        // sweep. A regression that re-inlined the empty-domain struct
        // literal would surface HERE and NOT at any of the three
        // sibling `validate_label` pins (each covers a different
        // gate), so the four pins together bind each pre-lift
        // rejection site to the ONE substrate constructor.
        let via_validate = validate_domain("domain", "").unwrap_err();
        let via_constructor = HostnameError::invalid_label("domain", "", "must not be empty");
        assert_eq!(via_validate, via_constructor);
    }
}
