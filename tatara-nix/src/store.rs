//! Content-addressed store — Nix's foundational abstraction, typed.
//!
//! A `StorePath` is a hash of the canonical inputs that produce it. Given
//! identical inputs, you get an identical path — that's the source of Nix's
//! determinism guarantees. We carry Nix's contract faithfully but use BLAKE3
//! (faster than SHA-256) and a typed `StoreHash` that prevents accidental
//! mixing with arbitrary byte strings.

use serde::{Deserialize, Serialize};

/// 256-bit BLAKE3 hash, hex-encoded (64 chars).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StoreHash(pub String);

impl StoreHash {
    /// Canonical hash over any `serde::Serialize` value.
    ///
    /// Delegates the two-line
    /// `serde_json::to_vec(value).unwrap_or_default()` +
    /// `hex::encode(blake3::hash(&bytes).as_bytes())` chain to
    /// [`tatara_lisp::hash::hex_blake3_of_json`] — the workspace-wide
    /// ONE substrate owner of the `T: Serialize` → 64-lowercase-hex
    /// BLAKE3 identity-string projection. The `StoreHash(...)` newtype
    /// wrap stays here (this slot owns its identity shape; the
    /// substrate owns the byte-projection).
    ///
    /// Byte-shape parity with the pre-lift hand-authored chain is
    /// pinned at
    /// [`tests::store_hash_of_matches_pre_lift_hand_authored_chain_bytewise`],
    /// so a regression in either the substrate composer or the
    /// pre-lift `hex::encode(blake3::hash(...).as_bytes())` reference
    /// surfaces HERE at fail-before-pass-after granularity.
    pub fn of<T: Serialize>(value: &T) -> Self {
        Self(tatara_lisp::hash::hex_blake3_of_json(value))
    }

    /// Truncated form — 20 hex chars (~80 bits) for human display only.
    pub fn short(&self) -> &str {
        &self.0[..20.min(self.0.len())]
    }
}

impl std::fmt::Display for StoreHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A content-addressed path in the tatara store.
/// Shape: `<StoreHash>-<name>[-<version>]`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StorePath {
    pub hash: StoreHash,
    pub name: String,
    pub version: Option<String>,
}

impl StorePath {
    pub fn new(hash: StoreHash, name: impl Into<String>, version: Option<String>) -> Self {
        Self {
            hash,
            name: name.into(),
            version,
        }
    }

    /// Canonical rendering: `<hash>-<name>[-<version>]` — mirrors Nix's store
    /// path shape while using our hash variant.
    pub fn render(&self) -> String {
        match &self.version {
            Some(v) => format!("{}-{}-{}", self.hash.short(), self.name, v),
            None => format!("{}-{}", self.hash.short(), self.name),
        }
    }
}

impl std::fmt::Display for StorePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic_and_64_hex() {
        let a = StoreHash::of(&"hello");
        let b = StoreHash::of(&"hello");
        assert_eq!(a, b);
        assert_eq!(a.0.len(), 64);
    }

    #[test]
    fn hash_differs_by_content() {
        assert_ne!(StoreHash::of(&"a"), StoreHash::of(&"b"));
    }

    #[test]
    fn store_path_renders_with_version() {
        let p = StorePath::new(StoreHash::of(&"x"), "hello", Some("2.12".into()));
        let rendered = p.render();
        assert!(rendered.ends_with("-hello-2.12"));
    }

    /// Fail-before-pass-after: [`StoreHash::of`]'s post-lift delegation
    /// to [`tatara_lisp::hash::hex_blake3_of_json`] MUST produce
    /// byte-identical output to the pre-lift hand-authored two-line
    /// `serde_json::to_vec(value).unwrap_or_default()` +
    /// `hex::encode(blake3::hash(&bytes).as_bytes())` chain that lived
    /// at this callsite. Every downstream consumer (StorePath render,
    /// InProcessRealizer cache-key, every `#[derive(TataraDomain)]`
    /// value threaded through the store) depends on this parity.
    ///
    /// A regression in either half — a substrate reshape (a switch
    /// away from `blake3::Hash::to_hex().to_string()`, a serde-json
    /// serializer swap) OR a hex/blake3 major-version bump reshaping
    /// the pre-lift reference — surfaces HERE at fail-before-pass-
    /// after granularity, before it silently invalidates every
    /// content-addressed [`StorePath`] on disk.
    #[test]
    fn store_hash_of_matches_pre_lift_hand_authored_chain_bytewise() {
        #[derive(Serialize)]
        struct Fixture {
            name: String,
            n: u32,
        }
        for value in [
            Fixture {
                name: "hello".into(),
                n: 0,
            },
            Fixture {
                name: "coreutils".into(),
                n: 9,
            },
            Fixture {
                name: String::new(),
                n: u32::MAX,
            },
        ] {
            let pre_lift = {
                let bytes = serde_json::to_vec(&value).unwrap_or_default();
                hex::encode(blake3::hash(&bytes).as_bytes())
            };
            assert_eq!(
                StoreHash::of(&value).0,
                pre_lift,
                "StoreHash::of drifted from pre-lift hand-authored chain for {:?}",
                value.name,
            );
        }
    }

    /// Cross-substrate parity: `StoreHash::of` and the
    /// `tatara_lisp::hash::hex_blake3_of_json` substrate resolve to
    /// byte-identical hex strings — the newtype wrap is the ONLY
    /// difference between them. Pins the wrapper's invariant so a
    /// future reshape of the newtype (a `#[repr(transparent)]` swap,
    /// an added prefix) surfaces at THIS pin.
    #[test]
    fn store_hash_of_body_matches_substrate_owner_bytewise() {
        #[derive(Serialize)]
        struct Fixture {
            k: &'static str,
        }
        let v = Fixture { k: "same" };
        assert_eq!(
            StoreHash::of(&v).0,
            tatara_lisp::hash::hex_blake3_of_json(&v)
        );
    }
}
