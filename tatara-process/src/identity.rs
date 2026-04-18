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
pub fn content_hash<T: Serialize>(spec: &T) -> String {
    let canonical = serde_json::to_vec(spec).unwrap_or_default();
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

/// Format a hierarchical process address: `{identity}.{pid_path}`.
///
/// Examples: `"seph.1"`, `"a3f7x9kp.1.1"`, `"seph.1.7.2"`.
pub fn format_process_address(identity: &Identity, pid_path: &str) -> String {
    format!("{}.{}", identity.name, pid_path)
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
        let h = content_hash(&Dummy { a: u32::MAX, b: "qwertyuiopasdfghjklzxcvbnm" });
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
}
