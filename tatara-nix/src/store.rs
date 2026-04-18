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
    pub fn of<T: Serialize>(value: &T) -> Self {
        let bytes = serde_json::to_vec(value).unwrap_or_default();
        Self(hex::encode(blake3::hash(&bytes).as_bytes()))
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
}
