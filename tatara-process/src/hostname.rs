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
//!   `akeyless-prod` or `pr-1234`.
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

/// Format the per-instance FQDN.
///
/// ```
/// use tatara_process::hostname::fmt_fqdn;
/// let fqdn = fmt_fqdn("gator", "akeyless-prod", "pleme-dev", "use1", "quero.lol").unwrap();
/// assert_eq!(fqdn, "gator.akeyless-prod.pleme-dev.use1.quero.lol");
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
/// let fqdn = fmt_fqdn_stable("gator", "pleme-dev", "use1", "quero.lol").unwrap();
/// assert_eq!(fqdn, "gator.pleme-dev.use1.quero.lol");
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
pub fn resolve_ephemeral_id<'a>(
    hostname: &'a RoutingHostname,
    fallback_hash: &'a str,
) -> &'a str {
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
    let hex = blake3::hash(bytes).to_hex().to_string();
    hex.chars().take(len).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn fmt_fqdn_per_instance() {
        let f = fmt_fqdn("gator", "akeyless-prod", "pleme-dev", "use1", "quero.lol").unwrap();
        assert_eq!(f, "gator.akeyless-prod.pleme-dev.use1.quero.lol");
    }

    #[test]
    fn fmt_fqdn_stable_form() {
        let f = fmt_fqdn_stable("gator", "pleme-dev", "use1", "quero.lol").unwrap();
        assert_eq!(f, "gator.pleme-dev.use1.quero.lol");
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
        assert!(matches!(r, Err(HostnameError::InvalidLabel { segment: "app", .. })));
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
        let spec = TestSpec { a: 1, b: "x".into() };
        let id = ephemeral_id_from_spec(&spec).unwrap();
        assert_eq!(id.len(), EPHEMERAL_ID_HASH_LEN);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn ephemeral_id_is_deterministic() {
        let s1 = TestSpec { a: 1, b: "x".into() };
        let s2 = TestSpec { a: 1, b: "x".into() };
        assert_eq!(
            ephemeral_id_from_spec(&s1).unwrap(),
            ephemeral_id_from_spec(&s2).unwrap()
        );
    }

    #[test]
    fn ephemeral_id_changes_with_spec() {
        let s1 = TestSpec { a: 1, b: "x".into() };
        let s2 = TestSpec { a: 2, b: "x".into() };
        let s3 = TestSpec { a: 1, b: "y".into() };
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
        let spec = TestSpec { a: 42, b: "anything".into() };
        let id = ephemeral_id_from_spec(&spec).unwrap();
        validate_label("ephemeral_id", &id).unwrap();
    }

    // ─── resolve_ephemeral_id ────────────────────────────────────

    #[test]
    fn resolve_named_slot_wins() {
        let h = RoutingHostname {
            app: "gator".into(),
            instance: Some("akeyless-prod".into()),
            cluster: None,
        };
        assert_eq!(resolve_ephemeral_id(&h, "fallback"), "akeyless-prod");
    }

    #[test]
    fn resolve_empty_named_falls_back() {
        let h = RoutingHostname {
            app: "gator".into(),
            instance: Some(String::new()),
            cluster: None,
        };
        assert_eq!(resolve_ephemeral_id(&h, "abc123de"), "abc123de");
    }

    #[test]
    fn resolve_unset_named_falls_back() {
        let h = RoutingHostname {
            app: "gator".into(),
            instance: None,
            cluster: None,
        };
        assert_eq!(resolve_ephemeral_id(&h, "abc123de"), "abc123de");
    }

    // ─── End-to-end ──────────────────────────────────────────────

    #[test]
    fn end_to_end_named_and_unnamed_for_same_process() {
        let spec = TestSpec { a: 1, b: "x".into() };
        let hash = ephemeral_id_from_spec(&spec).unwrap();

        let h_named = RoutingHostname {
            app: "gator".into(),
            instance: Some("akeyless-prod".into()),
            cluster: None,
        };
        let h_anon = RoutingHostname {
            app: "gateway".into(),
            instance: None,
            cluster: None,
        };

        let id_named = resolve_ephemeral_id(&h_named, &hash);
        let id_anon = resolve_ephemeral_id(&h_anon, &hash);

        let fqdn_named =
            fmt_fqdn(&h_named.app, id_named, "pleme-dev", "use1", "quero.lol").unwrap();
        let fqdn_anon =
            fmt_fqdn(&h_anon.app, id_anon, "pleme-dev", "use1", "quero.lol").unwrap();

        assert_eq!(fqdn_named, "gator.akeyless-prod.pleme-dev.use1.quero.lol");
        assert!(fqdn_anon.starts_with("gateway."));
        assert!(fqdn_anon.ends_with(".pleme-dev.use1.quero.lol"));
        // 5 named segments (app + eph_id + cluster + location + domain),
        // but `domain` itself splits as `quero.lol` ⇒ 6 dot-delimited
        // pieces. The shape, not the count, is the invariant.
        assert_eq!(fqdn_anon.matches('.').count(), 5);
    }
}
