//! Content-addressable identity — deterministic naming from spec.
//!
//! Every Process gets a 128-bit BLAKE3 hash of its canonical spec,
//! base32-encoded (26 chars) using an unambiguous alphabet (no 0/1/o/l).
//!
//! Ported from convergence-controller/src/identity.rs, generalized over
//! any `Serialize` spec (not just `ConvergenceProcessSpec`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Length of the truncated hash in bytes (128 bits of collision space).
const HASH_BYTES: usize = 16;

/// Crockford base32 alphabet — 32 chars, excludes `i/l/o/u` to remove the
/// most common visual collisions (1/l/i, 0/o, u/v). Matches Douglas
/// Crockford's published base32 spec.
const BASE32_ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// Resolved identity — human-assigned or content-derived.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Identity {
    /// The name used in the PID path (e.g., `"seph"` or `"a3f7x9kp2bfhmnqr5tvwxyzabc"`).
    pub name: String,
    /// Canonical-JSON BLAKE3 hash, 26-char base32. Always computed, even when overridden.
    pub content_hash: String,
    /// True when `name` came from `spec.identity.nameOverride`.
    pub name_override: bool,
}

/// Compute the content hash of any serializable spec.
///
/// The canonical pillar-bytes input rides through the ONE substrate
/// primitive [`crate::three_pillar::pillar_bytes`] — peer of the
/// intent-attestation-pillar consumers (`IntentVariant::canonical_bytes`,
/// `render::render_{flux,aplicacao,nix}`, `phase_machine::
/// compute_intent_hash`) that all route through the same
/// `serde_json::to_vec(v).unwrap_or_default()` shape. A future
/// upgrade of the pillar-bytes projection (a canonical-JSON
/// serializer for stable byte ordering, a size-cap guard, a serde-
/// error trace event before returning empty) lands at the substrate
/// owner and every content-hash + attestation consumer inherits the
/// upgrade mechanically.
pub fn content_hash<T: Serialize>(spec: &T) -> String {
    let canonical = crate::three_pillar::pillar_bytes(spec);
    let digest = blake3::hash(&canonical);
    base32_encode(&digest.as_bytes()[..HASH_BYTES])
}

/// Derive an identity from a spec + optional human override.
///
/// Override wins when non-empty; the content hash is always computed for integrity.
pub fn derive_identity<T: Serialize>(spec: &T, name_override: Option<&str>) -> Identity {
    let hash = content_hash(spec);
    match name_override.map(str::trim).filter(|s| !s.is_empty()) {
        Some(name) => Identity {
            name: name.to_string(),
            content_hash: hash,
            name_override: true,
        },
        None => Identity {
            name: hash.clone(),
            content_hash: hash,
            name_override: false,
        },
    }
}

/// Canonical hierarchical PID-path segment separator.
///
/// The ONE substrate owner of the `'.'` char every hierarchical-PID
/// composer + walker on this file + [`crate::pid`] (via
/// [`join_pid_segment`]) reaches through, so a future normalization of
/// the separator (a swap to `'/'` for a Unix-path-shaped rendering, a
/// per-segment escape for names carrying literal `.`s, a widened
/// grapheme-boundary walker for unicode-safe splits) lands at ONE
/// substrate primitive and every hierarchical-PID producer + consumer
/// picks up the upgrade mechanically.
///
/// Pre-lift the bare `'.'` char literal was hand-authored at TWO
/// production sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// threshold on the [`crate::pid`] side alone
/// ([`crate::pid::depth`] on `pid_path.split('.')` and
/// [`crate::pid::parent_of`] on `pid_path.rfind('.')`), plus the THREE
/// composer sites this module + [`crate::pid`] hand-authored on the
/// join axis before the [`join_pid_segment`] lift below routed them
/// through the same const.
///
/// Theory grounding: THEORY.md §II.1 invariant 4 (deterministic
/// identity — hierarchical PIDs are ONE cluster-wide address space
/// with ONE canonical separator; every producer + consumer of the
/// address space binds through the same substrate slot).
pub const PID_PATH_SEPARATOR: char = '.';

/// Compose the canonical `<head><PID_PATH_SEPARATOR><tail>`
/// hierarchical PID-path segment join.
///
/// Owns the fixed 2-slot `format!("{head}.{tail}")` shape as ONE
/// substrate site, routing the separator through
/// [`PID_PATH_SEPARATOR`] so a future normalization of the separator
/// (see the const's doc for the catalog) reaches every hierarchical-
/// PID producer through this ONE primitive.
///
/// Pre-lift the 2-slot join was hand-authored at THREE production
/// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold, split
/// across two crates:
/// - [`format_process_address`] on the (`identity.name`, `pid_path`)
///   pair — the root address composer.
/// - [`crate::pid::allocate_pid`] on the Some(parent) arm's
///   (`parent`, `next_sequence`) pair — the child PID allocator.
/// - [`crate::pid::allocate_pid`] on the None arm's
///   (`identity.name`, `next_sequence`) pair — the root PID allocator.
///
/// Post-lift each callsite reads
/// `join_pid_segment(<head>, <tail>)` and the composed byte shape
/// matches the pre-lift `format!` chain verbatim. The `tail`
/// parameter accepts any [`std::fmt::Display`]-able value so the
/// `u32 next_sequence` slot on [`crate::pid::allocate_pid`] flows
/// through the primitive without a pre-format `.to_string()` round-
/// trip, matching the sibling [`crate::boundary::Satisfaction::labeled_diagnostic`]
/// composer's `tail: impl Display` convention.
///
/// Extension: future hierarchical-PID producers (P3 kenshi-runner's
/// per-suite Job PID allocator, any future placement rule that names
/// a fresh child under an existing parent) land as ONE new callsite
/// through this composer instead of another hand-authored `format!(
/// "{head}.{tail}")` restatement.
///
/// Theory grounding: THEORY.md §VI.1 (generation over composition —
/// the shape recurred at three sites past the PRIME-DIRECTIVE ≥ 2
/// duplication trigger, and is lifted to ONE owner here). THEORY.md
/// §II.1 invariant 5 (composition preserves proofs — the three
/// callsites now compose structurally through ONE primitive; a
/// regression that drifted the separator at ONE site surfaces at
/// [`tests::join_pid_segment_*`] rather than as silent operator-
/// facing skew across the hierarchical-PID address space).
#[must_use]
pub fn join_pid_segment(head: &str, tail: impl std::fmt::Display) -> String {
    format!("{head}{PID_PATH_SEPARATOR}{tail}")
}

/// Format a hierarchical process address: `{identity}.{pid_path}`.
///
/// Examples: `"seph.1"`, `"a3f7x9kp.1.1"`, `"seph.1.7.2"`.
///
/// Routes through the ONE substrate composer [`join_pid_segment`] so a
/// future normalization of the hierarchical-PID join (see the composer's
/// doc for the catalog) reaches this address renderer mechanically.
pub fn format_process_address(identity: &Identity, pid_path: &str) -> String {
    join_pid_segment(&identity.name, pid_path)
}

fn base32_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity((bytes.len() * 8).div_ceil(5));
    let mut bits: u64 = 0;
    let mut n: u32 = 0;
    for &b in bytes {
        bits = (bits << 8) | u64::from(b);
        n += 8;
        while n >= 5 {
            n -= 5;
            out.push(BASE32_ALPHABET[((bits >> n) & 0x1f) as usize] as char);
        }
    }
    if n > 0 {
        out.push(BASE32_ALPHABET[((bits << (5 - n)) & 0x1f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Dummy {
        a: u32,
        b: &'static str,
    }

    #[test]
    fn content_hash_is_deterministic() {
        let s = Dummy { a: 1, b: "x" };
        assert_eq!(content_hash(&s), content_hash(&s));
    }

    #[test]
    fn content_hash_differs_for_different_input() {
        assert_ne!(
            content_hash(&Dummy { a: 1, b: "x" }),
            content_hash(&Dummy { a: 2, b: "x" })
        );
    }

    #[test]
    fn content_hash_length_is_26() {
        assert_eq!(content_hash(&Dummy { a: 0, b: "" }).len(), 26);
    }

    #[test]
    fn alphabet_excludes_ambiguous() {
        // Crockford base32 excludes i/l/o/u to eliminate visual collisions.
        let h = content_hash(&Dummy {
            a: u32::MAX,
            b: "qwertyuiopasdfghjklzxcvbnm",
        });
        for c in h.chars() {
            assert!(!matches!(c, 'i' | 'l' | 'o' | 'u'), "saw {c}");
        }
    }

    #[test]
    fn override_wins() {
        let id = derive_identity(&Dummy { a: 1, b: "x" }, Some("seph"));
        assert_eq!(id.name, "seph");
        assert!(id.name_override);
        assert_eq!(id.content_hash.len(), 26);
    }

    #[test]
    fn empty_override_falls_back_to_hash() {
        let id = derive_identity(&Dummy { a: 1, b: "x" }, Some("   "));
        assert!(!id.name_override);
        assert_eq!(id.name, id.content_hash);
    }

    #[test]
    fn address_format() {
        let id = Identity {
            name: "seph".into(),
            content_hash: "a".repeat(26),
            name_override: true,
        };
        assert_eq!(format_process_address(&id, "1.7"), "seph.1.7");
    }

    // ─── PID_PATH_SEPARATOR + join_pid_segment substrate pins ─────────
    //
    // The [`PID_PATH_SEPARATOR`] const + [`join_pid_segment`] composer
    // own the ONE substrate site every hierarchical-PID producer +
    // consumer across this module + [`crate::pid`] reaches through.
    // These pins bind both at fail-before-pass-after granularity so a
    // regression that drifted the separator char, changed the composer's
    // slot ordering (`{tail}{sep}{head}` typo), or dropped the routing
    // through the const surfaces HERE rather than as silent operator-
    // facing skew across the cluster-wide PID address space.

    #[test]
    fn pid_path_separator_is_dot() {
        // Byte-shape pin: the separator is `'.'`. A drift to `'/'`,
        // `':'`, or a widened grapheme separator would fail here rather
        // than as silent skew at every hierarchical-PID splitter
        // (`crate::pid::depth`, `crate::pid::parent_of`) + composer
        // (`join_pid_segment`).
        assert_eq!(PID_PATH_SEPARATOR, '.');
    }

    #[test]
    fn join_pid_segment_composes_head_then_separator_then_tail() {
        // Byte-shape pin: `join_pid_segment("seph", "1")` yields
        // `"seph.1"`, matching the pre-lift `format!("{}.{}",
        // identity.name, next_sequence)` shape at
        // `crate::pid::allocate_pid` (None arm).
        assert_eq!(join_pid_segment("seph", "1"), "seph.1");
    }

    #[test]
    fn join_pid_segment_composes_pid_path_tail_verbatim() {
        // Byte-shape pin: the `tail` slot accepts a `&str` carrying its
        // own inner separators without escaping — matches the pre-lift
        // `format_process_address` shape where the passed `pid_path` was
        // already a dot-delimited chain.
        assert_eq!(join_pid_segment("seph", "1.7"), "seph.1.7");
        assert_eq!(join_pid_segment("seph.1", "7"), "seph.1.7");
    }

    #[test]
    fn join_pid_segment_accepts_display_tail() {
        // Byte-shape pin: the `tail: impl Display` slot admits a `u32`
        // integer directly, matching the pre-lift `format!("{parent}.
        // {next_sequence}")` shape at `crate::pid::allocate_pid`
        // (Some(parent) arm) that inlined the `u32` slot without a
        // `.to_string()` round-trip.
        assert_eq!(join_pid_segment("seph.1", 7u32), "seph.1.7");
        // Sweep the whole hierarchical-PID next-sequence axis so a
        // regression at any single sequence value surfaces here.
        for seq in [0u32, 1, 42, u32::MAX] {
            assert_eq!(
                join_pid_segment("seph.1", seq),
                format!("seph.1.{seq}"),
                "join must match pre-lift `format!(\"{{parent}}.{{seq}}\")` for seq={seq}"
            );
        }
    }

    #[test]
    fn join_pid_segment_routes_through_pid_path_separator_const() {
        // Cross-primitive coherence pin: the composed body's separator
        // slot is byte-identical to the `PID_PATH_SEPARATOR` const. A
        // regression that inlined a bare `'.'` at the composer while
        // the const was renamed would surface HERE rather than as
        // silent skew between the two substrate primitives.
        let composed = join_pid_segment("seph", "1");
        let mut chars = composed.chars();
        assert_eq!(chars.next(), Some('s'));
        assert_eq!(chars.next(), Some('e'));
        assert_eq!(chars.next(), Some('p'));
        assert_eq!(chars.next(), Some('h'));
        assert_eq!(chars.next(), Some(PID_PATH_SEPARATOR));
        assert_eq!(chars.next(), Some('1'));
    }

    #[test]
    fn format_process_address_composes_through_join_pid_segment() {
        // Post-lift parity pin: `format_process_address` routes through
        // `join_pid_segment`, so its output for the same input is byte-
        // identical to the composer's output for the (identity.name,
        // pid_path) pair. A regression that inlined a bare `format!` at
        // the address renderer while the composer was upgraded would
        // silently split the two on the hierarchical-PID axis.
        let id = Identity {
            name: "seph".into(),
            content_hash: "a".repeat(26),
            name_override: true,
        };
        for pid_path in ["1", "1.7", "1.7.3"] {
            assert_eq!(
                format_process_address(&id, pid_path),
                join_pid_segment(&id.name, pid_path),
            );
        }
    }
}
