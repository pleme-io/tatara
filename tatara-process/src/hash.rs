//! Substrate primitive for BLAKE3 hex digests.
//!
//! Two peer entries — [`hex_blake3`] for the in-memory-buffer shape
//! (`hex::encode(blake3::hash(bytes).as_bytes())`) every three-pillar
//! attestation producer that fed BLAKE3 a single buffer restated by
//! hand pre-lift, and [`hex_blake3_hash`] for the streaming-digest
//! shape (`hex::encode(hash.as_bytes())` where `hash =
//! blake3::Hasher::finalize()`) every consumer that folded per-item
//! updates into a `Hasher` before finalizing walked. Both peers ride
//! through ONE hex-encoding step at [`hex_blake3_hash`] so a future
//! swap onto `blake3::Hash::to_hex().to_string()` (or a different
//! encoding — base32, base64url, uppercase hex for a downstream tool)
//! lands at ONE substrate function and every downstream three-pillar
//! consumer inherits the upgrade mechanically.
//!
//! Return type is `String` for wire-shape stability with the pre-lift
//! consumers — `ReceiptEnvelope.{intent,artifact,control}_hash` are
//! typed as `String`, so a `Cow`/`&str` return would force allocation
//! at every call site regardless.
//!
//! # Which peer to call
//!
//! - Have `&[u8]` in hand → [`hex_blake3`]. Internally it composes
//!   [`hex_blake3_hash`] over `blake3::hash(bytes)`.
//! - Have a `blake3::Hasher` you already folded per-item updates into
//!   → `hex_blake3_hash(&h.finalize())`. Skips the one-shot round-trip
//!   through `&[u8]` that would force the caller to materialize the
//!   full input buffer just to re-hash it.

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
///
/// Delegates the terminal `hex::encode(...as_bytes())` step to the
/// sibling [`hex_blake3_hash`] primitive so the encoding rule lives at
/// ONE substrate owner; a future re-encoding (base32, base64url,
/// uppercase, `blake3::Hash::to_hex()`) reaches BOTH the one-shot and
/// the streaming corner through ONE edit.
#[must_use]
pub fn hex_blake3(bytes: &[u8]) -> String {
    hex_blake3_hash(&blake3::hash(bytes))
}

/// Streaming-digest peer of [`hex_blake3`] — the ONE substrate owner
/// of the 1-link `hex::encode(hash.as_bytes())` encoding step every
/// three-pillar producer that folded per-item updates into a
/// `blake3::Hasher` walked pre-lift.
///
/// # Why it exists
///
/// The `Hasher::finalize() → hex::encode(<Hash>.as_bytes())` chain was
/// hand-authored at TWO sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold, each carrying a `Hasher` it fed per-item
/// updates into before finalizing:
///
/// * [`crate::three_pillar::compose_root`] — folds the domain tag +
///   the four pillars (artifact, control, intent, previous) into a
///   `blake3::Hasher`, then encodes the final hash. Pre-lift ended
///   with `hex::encode(h.finalize().as_bytes())` inline; post-lift
///   ends with `hex_blake3_hash(&h.finalize())`.
/// * `tatara-reconciler::phase_machine::handle_running` — folds each
///   observed FluxCD resource's `(apiVersion, kind, ns, name)`
///   4-slot identity into a `blake3::Hasher`, then encodes the final
///   hash as the artifact-pillar input for the ATTEST step.
///
/// Pre-lift the one-shot corner ([`hex_blake3`]) and the streaming
/// corner both restated `hex::encode(...as_bytes())` at their own
/// bodies, leaving a two-place drift trap — a future swap onto
/// `blake3::Hash::to_hex().to_string()` (a subtler pre-existing
/// spelling that appears at `crate::hostname::short_hex_blake3`), an
/// uppercase-hex flip for a downstream tool, or a base32-encoded
/// variant would have to land at BOTH sites or silently break receipt
/// verification at the corner that missed the update. Post-lift the
/// terminal hex step lives at ONE substrate owner and the one-shot
/// peer's body reduces to `hex_blake3_hash(&blake3::hash(bytes))`.
///
/// # Invariants
///
/// Same shape as [`hex_blake3`]: 64 chars of lowercase hex, every
/// char in `[0-9a-f]`, byte-identical to the pre-lift `hex::encode(<
/// hash>.as_bytes())` spelling.
///
/// # `#[must_use]`
///
/// Every consumer either stores the returned hex into a receipt
/// pillar or a `composed_root` slot. Dropping the return means the
/// caller finalized a `Hasher` for no observable reason — the
/// attribute surfaces that as a warning at every call site.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// `hex::encode(...as_bytes())` step recurred at two hand-authored
/// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and is
/// lifted to ONE owner here). THEORY.md §II.1 invariant 5
/// (composition preserves proofs — the pin
/// [`tests::hex_blake3_hash_matches_pre_lift_hex_encode_spelling_bytewise`]
/// binds the streaming corner byte-identically to the pre-lift
/// spelling, and the cross-corner pin
/// [`tests::hex_blake3_bytes_form_delegates_through_hex_blake3_hash`]
/// binds the one-shot peer to `hex_blake3_hash` so a regression in
/// either surfaces at ONE substrate pin rather than as silent
/// composed_root drift across every downstream consumer).
#[must_use]
pub fn hex_blake3_hash(hash: &blake3::Hash) -> String {
    hex::encode(hash.as_bytes())
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

    // ── hex_blake3_hash streaming-digest peer pins ─────────────────

    #[test]
    fn hex_blake3_hash_matches_pre_lift_hex_encode_spelling_bytewise() {
        // Byte-identical parity with the `hex::encode(<hash>.as_bytes())`
        // pre-lift spelling every three-pillar producer that
        // finalized a `Hasher` walked (compose_root's internal chain
        // + phase_machine's per-ref artifact-hash fold). Swept across
        // representative Hasher inputs so a substrate-side re-encoding
        // (a base32 flip, an uppercase-hex flip, a `to_hex().to_string()`
        // spelling drift) surfaces HERE rather than as silent
        // composed_root drift at every downstream consumer.
        for buf in [
            b"" as &[u8],
            b"x",
            b"hello",
            &[0u8; 64],
            &[0xFFu8; 128],
            b"tatara-process/v1alpha1\n",
            b"aaaa\ncccc\niiii\npppp",
        ] {
            let mut h = blake3::Hasher::new();
            h.update(buf);
            let hash = h.finalize();
            assert_eq!(
                hex_blake3_hash(&hash),
                hex::encode(hash.as_bytes()),
                "pre-lift `hex::encode(<hash>.as_bytes())` spelling drifted for buf.len()={}",
                buf.len(),
            );
        }
    }

    #[test]
    fn hex_blake3_hash_matches_pre_lift_to_hex_spelling_bytewise() {
        // Cross-spelling coherence with the alternate
        // `blake3::Hash::to_hex().to_string()` form used at
        // `tatara-process::hostname::short_hex_blake3`; both
        // spellings MUST produce byte-identical output so a future
        // consumer routed onto `hex_blake3_hash` cannot silently
        // diverge from a peer that still spells it the other way.
        for buf in [b"" as &[u8], b"x", b"hello", &[0xFFu8; 128]] {
            let mut h = blake3::Hasher::new();
            h.update(buf);
            let hash = h.finalize();
            assert_eq!(
                hex_blake3_hash(&hash),
                hash.to_hex().to_string(),
                "streaming `.to_hex().to_string()` spelling drifted for buf.len()={}",
                buf.len(),
            );
        }
    }

    #[test]
    fn hex_blake3_hash_output_is_lowercase_hex_of_blake3_length() {
        // BLAKE3 produces 32-byte digests; hex-encoded → 64 lowercase
        // characters. Pin the output shape so a downstream reader's
        // width assumption (a 26-char base32 slot in the wire form,
        // for instance) surfaces here rather than as a wire-parse
        // failure downstream.
        let mut h = blake3::Hasher::new();
        h.update(b"pillar-input");
        let out = hex_blake3_hash(&h.finalize());
        assert_eq!(out.len(), 64);
        assert!(out.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(out.chars().all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn hex_blake3_bytes_form_delegates_through_hex_blake3_hash() {
        // Cross-primitive coherence — the one-shot [`hex_blake3`]
        // peer MUST agree byte-for-byte with the streaming peer
        // composed over `blake3::hash(bytes)`. A regression that
        // specialized ONE peer (a different encoding, a
        // canonicalization step) would surface HERE rather than as
        // silent drift between the one-shot and streaming corners
        // at every downstream consumer.
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
                hex_blake3_hash(&blake3::hash(buf)),
                "one-shot `hex_blake3` drifted from streaming peer for buf.len()={}",
                buf.len(),
            );
        }
    }

    #[test]
    fn hex_blake3_hash_is_deterministic_across_calls() {
        // BLAKE3 is deterministic; the encoder is pure. A regression
        // that accidentally seeded a nonce, read a clock, or salted
        // the encoding would fail loudly HERE rather than as silent
        // composed_root drift at every downstream consumer.
        let mut h = blake3::Hasher::new();
        h.update(b"input");
        let hash = h.finalize();
        assert_eq!(hex_blake3_hash(&hash), hex_blake3_hash(&hash));
    }
}
