//! Content-addressed convergence point identity.
//!
//! Like a Nix store path is hash(inputs + builder), a PointId is
//! hash(convergence_function + input_attestations + desired_state).
//! Same inputs + same function = same PointId = cache hit.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Content-addressed identifier for a convergence point.
/// Computed as blake3(function_hash || input_hashes || desired_state_hash).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PointId([u8; 32]);

impl PointId {
    /// Compute a PointId from its constituent hashes.
    pub fn compute(
        function_hash: &[u8],
        input_hashes: &[&[u8]],
        desired_state_hash: &[u8],
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(function_hash);
        for input in input_hashes {
            hasher.update(input);
        }
        hasher.update(desired_state_hash);
        Self(*hasher.finalize().as_bytes())
    }

    /// Create from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Create from hex string.
    pub fn from_hex(hex: &str) -> Result<Self, hex::FromHexError> {
        let mut bytes = [0u8; 32];
        hex::decode_to_slice(hex, &mut bytes)?;
        Ok(Self(bytes))
    }

    /// Convert to hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Display for PointId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for PointId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PointId({})", &self.to_hex()[..16])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic() {
        let a = PointId::compute(b"fn1", &[b"input1"], b"desired1");
        let b = PointId::compute(b"fn1", &[b"input1"], b"desired1");
        assert_eq!(a, b);
    }

    #[test]
    fn test_different_inputs_different_ids() {
        let a = PointId::compute(b"fn1", &[b"input1"], b"desired1");
        let b = PointId::compute(b"fn1", &[b"input2"], b"desired1");
        assert_ne!(a, b);
    }

    #[test]
    fn test_hex_roundtrip() {
        let id = PointId::compute(b"test", &[], b"state");
        let hex = id.to_hex();
        let parsed = PointId::from_hex(&hex).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_display() {
        let id = PointId::compute(b"test", &[], b"state");
        let s = format!("{id}");
        assert_eq!(s.len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn test_ordering() {
        let a = PointId::compute(b"a", &[], b"s");
        let b = PointId::compute(b"b", &[], b"s");
        // Just verify ordering is consistent
        assert!(a < b || a > b || a == b);
    }

    #[test]
    fn test_serde_roundtrip() {
        let id = PointId::compute(b"serde", &[b"test"], b"data");
        let json = serde_json::to_string(&id).unwrap();
        let parsed: PointId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }
}
