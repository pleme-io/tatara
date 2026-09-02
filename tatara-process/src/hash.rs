//! Substrate primitive for BLAKE3 hex digests over a byte buffer.
//!
//! Owns the 2-link `hex::encode(blake3::hash(bytes).as_bytes())`
//! (equivalently `blake3::hash(bytes).to_hex().to_string()`) shape
//! every downstream three-pillar-attestation producer restated by
//! hand pre-lift.
//!
//! Return type is `String` for wire-shape stability with the pre-lift
//! consumers — `ReceiptEnvelope.{intent,artifact,control}_hash` are
//! typed as `String`, so a `Cow`/`&str` return would force allocation
//! at every call site regardless.
//!
//! The primitive is single-shot over an in-memory buffer. A streaming
//! digest (`blake3::Hasher::finalize()`) is a peer with different
//! ownership; a `hex_blake3_hash(&blake3::Hash)` sibling can land on
//! this module the day a second consumer wants to reuse the same hex
//! encoding without re-hashing.

/// Compute the lowercase 64-char BLAKE3 hex digest of `bytes`.
///
/// # Invariants
///
/// - **Length:** the returned string is always exactly 64 chars
///   (BLAKE3's 32-byte digest encoded as lowercase hex).
/// - **Charset:** every char is one of `[0-9a-f]` (lowercase).
/// - **Determinism:** byte-identical output across runs for the same
///   input; matches both the `hex::encode(blake3::hash(x).as_bytes())`
///   and `blake3::hash(x).to_hex().to_string()` pre-lift spellings.
///
/// # `#[must_use]`
///
/// Every consumer either stores the returned hex into a receipt
/// pillar (`intent_hash`, `artifact_hash`, `control_hash`) or feeds
/// it into a DNS slot / stable name. Dropping the return means the
/// hash was computed for no observable reason — the attribute
/// surfaces that as a warning at every call site.
#[must_use]
pub fn hex_blake3(bytes: &[u8]) -> String {
    hex::encode(blake3::hash(bytes).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_blake3_returns_64_char_lowercase_hex() {
        let d = hex_blake3(b"hello");
        assert_eq!(d.len(), 64, "BLAKE3 digest hex-encodes to 64 chars");
        assert!(
            d.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "digest must be lowercase hex: {d}"
        );
    }

    #[test]
    fn hex_blake3_is_deterministic() {
        assert_eq!(hex_blake3(b"hello"), hex_blake3(b"hello"));
        assert_ne!(hex_blake3(b"hello"), hex_blake3(b"world"));
    }

    #[test]
    fn hex_blake3_empty_input_matches_known_digest() {
        // Known BLAKE3 digest of the empty input — a rename of the
        // underlying algo (or an accidental salting) would land here
        // rather than as silent receipt-root drift across every
        // three-pillar consumer.
        assert_eq!(
            hex_blake3(b""),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
        );
    }

    #[test]
    fn hex_blake3_matches_pre_lift_hex_encode_spelling_bytewise() {
        // Byte-identical parity with the `hex::encode(blake3::hash(x).as_bytes())`
        // pre-lift spelling used at every reconciler + probe + worker
        // consumer routed onto this primitive; guards against a
        // substrate-side canonicalization the pre-lift chain does NOT
        // apply.
        for buf in [
            b"" as &[u8],
            b"x",
            b"hello",
            &[0u8; 256],
            b"tatara-receipt/v1",
            b"{\"kind\":\"tatara.export\"}",
        ] {
            assert_eq!(
                hex_blake3(buf),
                hex::encode(blake3::hash(buf).as_bytes()),
                "pre-lift `hex::encode(...)` spelling drifted for buf.len()={}",
                buf.len(),
            );
        }
    }

    #[test]
    fn hex_blake3_matches_pre_lift_to_hex_spelling_bytewise() {
        // Byte-identical parity with the alternate `blake3::hash(x).to_hex().to_string()`
        // pre-lift spelling used at `tatara-process::hostname::short_hex_blake3`,
        // `tatara-export-worker::hex_blake3`, and the `p2p::chunk::blake3_hash`
        // helpers; catches a divergence between the two workspace
        // spellings that would otherwise silently break stable-name /
        // ephemeral-id / receipt-root parity at any consumer that
        // still spelled it the other way.
        for buf in [
            b"" as &[u8],
            b"x",
            b"hello",
            &[0xFFu8; 128],
            b"pleme-dev/ephemeral-test-01",
        ] {
            assert_eq!(
                hex_blake3(buf),
                blake3::hash(buf).to_hex().to_string(),
                "pre-lift `.to_hex().to_string()` spelling drifted for buf.len()={}",
                buf.len(),
            );
        }
    }
}
