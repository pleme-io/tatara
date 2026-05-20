//! Deterministic pool-member naming.
//!
//! Pool member Process names are `<pool>-<short-blake3>` where the
//! short hash is derived from `(pool.uid, slot_ordinal)`. Deterministic
//! so re-reconciling the same pool spec doesn't churn names; short
//! enough to fit inside K8s 253-byte name limits even with prefixes.

/// Produce a deterministic 8-char hex slug for a pool slot.
#[must_use]
pub fn slot_slug(pool_uid: &str, slot_ordinal: u32) -> String {
    let mut h = blake3::Hasher::new();
    h.update(pool_uid.as_bytes());
    h.update(b"\n");
    h.update(slot_ordinal.to_be_bytes().as_slice());
    let bytes = h.finalize();
    let hex = hex::encode(&bytes.as_bytes()[..4]);
    hex[..8].to_string()
}

/// Compose a full Process name from pool + slot.
#[must_use]
pub fn member_process_name(pool_name: &str, pool_uid: &str, slot_ordinal: u32) -> String {
    let slug = slot_slug(pool_uid, slot_ordinal);
    let max_pool_prefix = 245 - slug.len();
    let prefix = if pool_name.len() > max_pool_prefix {
        &pool_name[..max_pool_prefix]
    } else {
        pool_name
    };
    format!("{prefix}-{slug}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_deterministic() {
        assert_eq!(slot_slug("uid-1", 0), slot_slug("uid-1", 0));
        assert_ne!(slot_slug("uid-1", 0), slot_slug("uid-1", 1));
        assert_ne!(slot_slug("uid-1", 0), slot_slug("uid-2", 0));
    }

    #[test]
    fn slug_is_eight_hex_chars() {
        let s = slot_slug("uid", 42);
        assert_eq!(s.len(), 8);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn member_name_truncates_long_pool_name_to_dns_limit() {
        let long_pool = "x".repeat(300);
        let n = member_process_name(&long_pool, "uid", 0);
        assert!(n.len() <= 253);
        assert!(n.contains('-'));
    }

    #[test]
    fn member_name_composes_pool_and_slug() {
        let n = member_process_name("akeyless-pool", "uid-x", 5);
        assert!(n.starts_with("akeyless-pool-"));
        assert_eq!(n.len(), "akeyless-pool-".len() + 8);
    }
}
