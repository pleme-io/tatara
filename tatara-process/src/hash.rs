//! Substrate primitive for BLAKE3 hex digests.
//!
//! Two peer entries — [`hex_blake3`] for the in-memory-buffer shape
//! every three-pillar attestation producer that fed BLAKE3 a single
//! buffer restated by hand pre-lift, and [`hex_blake3_hash`] for the
//! streaming-digest shape (a finalized `blake3::Hasher` handle) every
//! consumer that folded per-item updates into a `Hasher` before
//! finalizing walked.
//!
//! # Crate-layer partition — both peers delegate through `tatara-lisp`
//!
//! Both entry points now delegate through the workspace-wide substrate
//! owners at the `tatara-lisp` layer:
//!
//! - [`hex_blake3`] → [`tatara_lisp::hash::hex_blake3_of_bytes`] — the
//!   byte-input owner (commit `524e543`, sibling of `hex_blake3_of_json`).
//! - [`hex_blake3_hash`] → [`tatara_lisp::hash::hex_blake3_of_hash`] —
//!   the streaming-input owner opened alongside the byte-input owner as
//!   the peer on the `&blake3::Hash → 64-hex` axis.
//!
//! Both crate-layer peers share ONE canonical spelling via those
//! substrates: `blake3::hash(bytes).to_hex().to_string()` and
//! `<hash>.to_hex().to_string()` respectively — byte-identical output
//! to the pre-lift `hex::encode(...)` chains through the `to_hex` /
//! `hex::encode` byte-shape parity pinned at each substrate. A future
//! encoding change (base32, base64url, uppercase hex for a downstream
//! tool) at either `tatara_lisp::hash` owner reaches every workspace
//! consumer — the three `tatara-closed-loop-probe` receipt pillars,
//! the `tatara-reconciler` `render`/`phase_machine` composers, the
//! `three_pillar::compose_root` write tail, `tatara-export-worker`'s
//! event-run digest, `crate::hostname::short_hex_blake3` — through
//! ONE edit. Pre-lift the workspace had two independent owners on the
//! byte-input axis and TWO independent spellings on the streaming-
//! input axis (`hex::encode(<hash>.as_bytes())` at this crate's
//! [`hex_blake3_hash`] and the byte-shape-equivalent
//! `.to_hex().to_string()` inlined inside `tatara-lisp`'s
//! `hex_blake3_of_bytes` body); post-lift both axes share ONE canonical
//! spelling by construction.
//!
//! Return type is `String` for wire-shape stability with the pre-lift
//! consumers — `ReceiptEnvelope.{intent,artifact,control}_hash` are
//! typed as `String`, so a `Cow`/`&str` return would force allocation
//! at every call site regardless.
//!
//! # Which peer to call
//!
//! - Have `&[u8]` in hand → [`hex_blake3`]. Delegates to
//!   [`tatara_lisp::hash::hex_blake3_of_bytes`], the workspace-wide
//!   byte-input owner.
//! - Have a `blake3::Hasher` you already folded per-item updates into
//!   → `hex_blake3_hash(&h.finalize())`. Delegates to
//!   [`tatara_lisp::hash::hex_blake3_of_hash`], the workspace-wide
//!   streaming-input owner. Skips the one-shot round-trip through
//!   `&[u8]` that would force the caller to materialize the full
//!   input buffer just to re-hash it.

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
/// # Delegation
///
/// Routes through [`tatara_lisp::hash::hex_blake3_of_bytes`] — the
/// workspace-wide byte-input owner (commit `524e543` opened it at the
/// `tatara-lisp` layer as the sibling of `hex_blake3_of_json` on the
/// value-input axis). Pre-lift the workspace had TWO independent
/// byte-input owners: this crate's `hex_blake3` keyed off
/// `hex::encode(<hash>.as_bytes())`, and `tatara-lisp`'s
/// `hex_blake3_of_bytes` keyed off `.to_hex().to_string()`. Both
/// produced byte-identical output but through two spellings, so a
/// future encoding change would have to land at both sites or
/// silently break receipt / stable-name parity at the corner that
/// missed the update. Post-lift the byte-input axis has ONE canonical
/// owner, and BOTH this crate's peer and `tatara-lisp`'s peer share
/// its spelling by construction — a future re-encoding (base32,
/// base64url, uppercase, an alternate `to_hex` shape on a future
/// blake3 major-version bump) lands at ONE substrate function and
/// reaches every downstream three-pillar / receipt / hostname /
/// export-run identity slot mechanically.
///
/// The streaming peer [`hex_blake3_hash`] cannot fold into the same
/// owner because it takes `&blake3::Hash` rather than `&[u8]` — a
/// structurally distinct entry point for consumers that folded
/// per-item updates into a `Hasher` before finalizing. Both peers
/// still agree byte-for-byte (pinned at
/// [`tests::hex_blake3_bytes_form_delegates_through_hex_blake3_hash`]
/// on the two-corner axis, and at
/// [`tests::hex_blake3_delegates_through_tatara_lisp_hex_blake3_of_bytes`]
/// on the cross-crate axis).
#[must_use]
pub fn hex_blake3(bytes: &[u8]) -> String {
    tatara_lisp::hash::hex_blake3_of_bytes(bytes)
}

/// Streaming-digest peer of [`hex_blake3`] — the crate-layer entry
/// point on the `&blake3::Hash → 64-hex` axis for every three-pillar
/// producer that folded per-item updates into a `blake3::Hasher`
/// before finalizing.
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
/// # Delegation
///
/// Routes through [`tatara_lisp::hash::hex_blake3_of_hash`] — the
/// workspace-wide streaming-input owner opened alongside the byte-
/// input owner `hex_blake3_of_bytes` at the `tatara-lisp` layer as
/// its peer on the `&blake3::Hash → 64-hex` axis. Pre-lift the
/// workspace had TWO independent spellings on this axis: this crate's
/// `hex_blake3_hash` keyed off `hex::encode(<hash>.as_bytes())`, and
/// the byte-shape-equivalent `.to_hex().to_string()` spelling inlined
/// inside `tatara-lisp`'s `hex_blake3_of_bytes` body. Both produced
/// byte-identical output but through two spellings; a future encoding
/// change would have to land at both sites or silently break receipt/
/// composed-root parity at the corner that missed the update.
/// Post-lift the streaming-input axis has ONE canonical owner
/// (`tatara_lisp::hash::hex_blake3_of_hash`), and BOTH this crate's
/// peer and `tatara-lisp`'s byte-input peer share its spelling by
/// construction — a future re-encoding (base32, base64url, uppercase,
/// an alternate `to_hex` shape on a future blake3 major-version bump)
/// lands at ONE substrate function and reaches every downstream three-
/// pillar / composed-root / stable-name / receipt identity slot
/// mechanically.
///
/// The one-shot peer [`hex_blake3`] still cannot fold into the
/// streaming owner because it takes `&[u8]` rather than `&blake3::Hash`
/// — a structurally distinct entry point for consumers that hand in
/// raw bytes without a pre-computed `blake3::Hash`. Both peers still
/// agree byte-for-byte (pinned at
/// [`tests::hex_blake3_bytes_form_delegates_through_hex_blake3_hash`]
/// on the two-corner axis, and at
/// [`tests::hex_blake3_hash_delegates_through_tatara_lisp_hex_blake3_of_hash`]
/// on the cross-crate axis).
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
/// streaming-input `blake3::Hash → 64-hex` encoding step now lives at
/// ONE substrate owner across the workspace, and this crate's peer
/// delegates through it rather than re-authoring the spelling).
/// THEORY.md §II.1 invariant 5 (composition preserves proofs — the
/// pin
/// [`tests::hex_blake3_hash_matches_pre_lift_hex_encode_spelling_bytewise`]
/// binds the streaming corner byte-identically to the pre-lift
/// spelling, and the cross-crate pin
/// [`tests::hex_blake3_hash_delegates_through_tatara_lisp_hex_blake3_of_hash`]
/// binds the delegation so a regression at either site surfaces at ONE
/// substrate pin rather than as silent composed_root drift across
/// every downstream consumer).
#[must_use]
pub fn hex_blake3_hash(hash: &blake3::Hash) -> String {
    tatara_lisp::hash::hex_blake3_of_hash(hash)
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

    // ── cross-crate byte-input owner delegation pin ────────────────

    /// Fail-before-pass-after: this crate's byte-input peer
    /// [`hex_blake3`] MUST agree byte-for-byte with the workspace-wide
    /// byte-input owner [`tatara_lisp::hash::hex_blake3_of_bytes`] on
    /// every observable input. Pre-lift the two owners lived side-by-
    /// side with distinct internal spellings (this crate walked
    /// `hex::encode(<hash>.as_bytes())`; tatara-lisp walked
    /// `.to_hex().to_string()`), producing identical output but
    /// through two independent code paths. Post-lift this peer
    /// DELEGATES through the tatara-lisp owner, so the two byte-input
    /// entry points across the workspace share ONE canonical spelling
    /// by construction. A regression that specialized ONE peer (a
    /// per-fleet canonicalization step at either site, an encoding
    /// swap at only one owner, a future spelling change that landed
    /// at the tatara-process peer but not the tatara-lisp owner or
    /// vice versa) would surface HERE — not as silent identity-slot
    /// drift across every downstream consumer that crosses the crate
    /// boundary between the two byte-input axes.
    ///
    /// Swept across the same representative buffer shapes the sibling
    /// pin [`hex_blake3_matches_pre_lift_hex_encode_spelling_bytewise`]
    /// walks, so a byte-shape regression at either end of the two-crate
    /// axis surfaces at the same corners the pre-lift parity pin
    /// covered.
    #[test]
    fn hex_blake3_delegates_through_tatara_lisp_hex_blake3_of_bytes() {
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
                tatara_lisp::hash::hex_blake3_of_bytes(buf),
                "tatara-process::hash::hex_blake3 drifted from \
                 tatara_lisp::hash::hex_blake3_of_bytes for buf.len()={}",
                buf.len(),
            );
        }
    }

    /// Fail-before-pass-after: this crate's streaming-input peer
    /// [`hex_blake3_hash`] MUST agree byte-for-byte with the workspace-
    /// wide streaming-input owner [`tatara_lisp::hash::hex_blake3_of_hash`]
    /// on every observable `blake3::Hash` handle. Pre-lift this crate
    /// walked `hex::encode(<hash>.as_bytes())` inline while `tatara-lisp`
    /// inlined the byte-shape-equivalent `.to_hex().to_string()`
    /// spelling inside `hex_blake3_of_bytes`; the two owners produced
    /// identical output but through two independent code paths on the
    /// `&blake3::Hash → 64-hex` axis. Post-lift this peer DELEGATES
    /// through the tatara-lisp owner, so the workspace's streaming and
    /// byte-input hex-encode axes share ONE canonical spelling by
    /// construction. A regression that specialized ONE peer (a per-
    /// fleet canonicalization step at either site, an encoding swap at
    /// only one owner, a future spelling change that landed at the
    /// tatara-process peer but not the tatara-lisp owner or vice versa)
    /// surfaces HERE — not as silent composed-root / artifact-hash
    /// drift across every downstream three-pillar / ATTEST-phase
    /// consumer that crosses the crate boundary. Swept across
    /// representative `blake3::Hasher` inputs so a byte-shape regression
    /// at either end of the two-crate axis surfaces at the same corners
    /// the sibling pin
    /// [`hex_blake3_hash_matches_pre_lift_hex_encode_spelling_bytewise`]
    /// covers.
    #[test]
    fn hex_blake3_hash_delegates_through_tatara_lisp_hex_blake3_of_hash() {
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
                tatara_lisp::hash::hex_blake3_of_hash(&hash),
                "tatara-process::hash::hex_blake3_hash drifted from \
                 tatara_lisp::hash::hex_blake3_of_hash for buf.len()={}",
                buf.len(),
            );
        }
    }
}
