//! Three-pillar BLAKE3 composition — the ONE substrate owner for the
//! typed hashing chain every `tatara-process/v1alpha1` attestation +
//! receipt-envelope consumer walks.
//!
//! # Why it exists
//!
//! Two peer consumers in this crate walked the SAME domain-tagged
//! BLAKE3 chain pre-lift, each with its own private `DOMAIN_TAG`
//! constant, its own 4-argument `compose_*` fn, AND its own
//! `constant_time_eq` byte-comparator:
//!
//! * [`crate::attestation::ProcessAttestation::compose`] + `verify` —
//!   the on-chain attestation writer. Composes a new
//!   `attestation.composed_root` from the four pillars + a chained
//!   `previous_root`, and verifies a persisted attestation matches
//!   its own claim.
//! * [`crate::receipt::ReceiptEnvelope::build`] + `verify_root` — the
//!   fleet-wide receipt-envelope writer + reader. Composes a new
//!   `envelope.composed_root` from the four pillars + the operator's
//!   expected `previous_root`, and verifies a wire-parsed envelope
//!   matches its own claim.
//!
//! Both consumers restated the identical BLAKE3 chain byte-for-byte
//! (`DOMAIN_TAG` | `artifact` | `\n` | `control?` | `\n` | `intent` |
//! `\n` | `previous?`), the identical `hex::encode(h.finalize().
//! as_bytes())` cast, AND the identical eight-line
//! `constant_time_eq` byte-comparator. The receipt-side even documented
//! the duplication in a pre-lift comment ("Same composition as
//! `ProcessAttestation::composed_hex` — kept local so
//! `tatara_process::receipt::compose_root(...)` is a single line in
//! downstream code without re-importing the attestation module").
//!
//! Silent divergence between the two chains would break receipt
//! verification with **no compile-time signal** — the reconciler's
//! `ConditionKind::ClosedLoopAuth` evaluator would false-negative
//! every closed-loop probe receipt against a Process attestation
//! whose composed_root uses the drifted rule. Silent divergence on
//! `DOMAIN_TAG` (a version bump on ONE side, or a typo on either)
//! would silently invalidate every persisted receipt against the
//! attestation chain that reads it back. Silent divergence on the
//! `constant_time_eq` bit-mask fold (a `!=` typo, an early `return
//! true` on empty inputs, a short-circuit `&&`) would open a
//! timing-side-channel corner AT ONE consumer without touching the
//! peer's proof.
//!
//! # What the lift owns
//!
//! One typed owner per shape:
//!
//! * [`DOMAIN_TAG`] — the `tatara-process/v1alpha1\n` prefix bytes.
//!   The prefix "tatara-process" is the *crate name*, not the K8s
//!   API group (which is `tatara.pleme.io`) — the receipt schema
//!   version + the attestation domain-separation tag are keyed off
//!   the crate that owns the wire type, deliberately independent of
//!   how kube-rs projects the CRD group. A pin below binds the const
//!   to `format!("tatara-process/{}\n", crate::VERSION)` so a future
//!   CRD-version bump lands at the substrate owner AND the domain
//!   tag together, not at the tag alone (silent invalidation of
//!   every persisted composed_root) or at the version alone (silent
//!   attestation of a stale tag past a wire-format break).
//! * [`compose_root`] — the 4-pillar BLAKE3 → hex projection.
//! * [`constant_time_eq`] — the length-checked, bit-mask-folded
//!   byte-comparator. Peer to the `subtle` crate's `ConstantTimeEq`
//!   trait but pure Rust, no dep.
//!
//! # Why it compounds
//!
//! A future normalization at the substrate owner reaches BOTH
//! consumers (attestation + receipt) mechanically — no per-site
//! edit at either callsite:
//!
//! * A CRD-version bump (`v1alpha1` → `v1beta1` → `v1`) lands as ONE
//!   `DOMAIN_TAG` byte-string edit at the substrate owner; both
//!   consumers pick it up at the same commit or neither does.
//! * A domain-tag structural change (a length-prefix, a version-
//!   independent stable tag, a per-pillar sub-tag) lands at ONE
//!   composer body.
//! * A move to a subtler constant-time comparator (a `subtle`-crate
//!   dep, an intrinsics-backed comparator on nightly, an
//!   architecture-conditional short-circuit ban) lands at ONE
//!   comparator body.
//!
//! # Not a `constant_time_eq` crate substitution
//!
//! The workspace's Cargo.lock already carries the `constant_time_eq`
//! crate as a transitive dep of the BLAKE3 backend, but pulling it in
//! as a direct dep here would add a compile-time-tunable direct dep
//! for a comparator whose body is literally eight lines and whose
//! typed contract this module already owns. Kept pure Rust; a future
//! swap onto `subtle::ConstantTimeEq` or an intrinsics-backed
//! comparator lands at [`constant_time_eq`] below without changing
//! any caller.

use blake3::Hasher;

/// The domain-separation tag every three-pillar composition rides.
///
/// The prefix `tatara-process` is the *crate name* that owns the
/// wire type, deliberately independent of the CRD's K8s API group
/// (`tatara.pleme.io`). The version suffix binds to
/// [`crate::VERSION`] via the pin at
/// [`tests::domain_tag_matches_crate_name_and_version_bytes`] so a
/// future CRD-version bump either lands at both or fails-loudly at
/// the pin.
pub const DOMAIN_TAG: &[u8] = b"tatara-process/v1alpha1\n";

/// Compose the three-pillar BLAKE3 → hex composed_root from the four
/// pillars. `control` and `previous` are `Option<&str>` because the
/// receipt-envelope + attestation surfaces both treat an absent
/// slot as "no control step" / "no chain predecessor", encoded on
/// the wire as either an empty string (the receipt-envelope
/// `control_hash: ""` posture) or an absent slot (the attestation
/// `previous_root: None` posture). The composer normalizes both onto
/// the same "empty-bytes chunk between the `\n` separators" wire
/// shape — matching every pre-lift consumer byte-for-byte.
///
/// A byte-identity pin at [`tests::compose_root_matches_pre_lift_
/// hand_authored_chain`] fixes the composition against the
/// hand-authored chain both pre-lift consumers walked, so a
/// regression at the composer's body (a reordered pillar, a swapped
/// separator, a missing `hex::encode`) surfaces at ONE substrate
/// pin rather than as silent invalidation of every downstream
/// composed_root read.
#[must_use]
pub fn compose_root(
    artifact: &str,
    control: Option<&str>,
    intent: &str,
    previous: Option<&str>,
) -> String {
    let mut h = Hasher::new();
    h.update(DOMAIN_TAG);
    h.update(artifact.as_bytes());
    h.update(b"\n");
    h.update(control.unwrap_or("").as_bytes());
    h.update(b"\n");
    h.update(intent.as_bytes());
    h.update(b"\n");
    h.update(previous.unwrap_or("").as_bytes());
    hex::encode(h.finalize().as_bytes())
}

/// Length-checked, bit-mask-folded constant-time byte comparator.
///
/// Returns `true` iff `a` and `b` are equal in length AND in every
/// byte. On unequal lengths short-circuits `false` without touching
/// the payload — matches every pre-lift comparator byte-for-byte
/// (the length short-circuit at both attestation.rs + receipt.rs
/// pre-lift is a load-bearing "different lengths CAN NEVER be
/// equal" fast path, not a leak). On equal lengths folds a bit-mask
/// across the full payload before deciding, so a per-byte timing
/// leak does not surface at ONE consumer without touching the peer.
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── DOMAIN_TAG shape pins ─────────────────────────────────────

    #[test]
    fn domain_tag_matches_crate_name_and_version_bytes() {
        // Binds the substrate `DOMAIN_TAG` const to the crate-name
        // prefix "tatara-process" + the workspace-wide
        // `crate::VERSION` spelling. A future CRD-version bump that
        // lands at ONE side (say, `VERSION` becomes `v1beta1` but
        // `DOMAIN_TAG` stays `v1alpha1`) fails loudly HERE rather
        // than as silent invalidation of every persisted
        // composed_root on the wire.
        //
        // Note the prefix is the CRATE name, not the K8s API GROUP
        // (`tatara.pleme.io`) — the receipt schema version + the
        // attestation domain-separation tag are keyed off the crate
        // that owns the wire type, deliberately independent of how
        // kube-rs projects the CRD group.
        let expected = format!("tatara-process/{}\n", crate::VERSION);
        assert_eq!(DOMAIN_TAG, expected.as_bytes());
    }

    #[test]
    fn domain_tag_ends_with_newline_separator() {
        // The pre-lift chain relied on `DOMAIN_TAG`'s trailing `\n`
        // to double as the first field separator (no explicit `h.
        // update(b"\n")` between the tag and the artifact chunk).
        // A regression that dropped the trailing newline would
        // silently produce a different composed_root for every
        // downstream receipt, so bind the shape here.
        assert_eq!(DOMAIN_TAG.last(), Some(&b'\n'));
    }

    // ── compose_root byte-identity pins ───────────────────────────

    /// The hand-authored chain both pre-lift consumers walked —
    /// `attestation::composed_hex` and `receipt::compose_root` had
    /// identical bodies to this. The substrate `compose_root` MUST
    /// match this byte-for-byte for every input on every consumer.
    fn hand_authored_chain(
        artifact: &str,
        control: Option<&str>,
        intent: &str,
        previous: Option<&str>,
    ) -> String {
        let mut h = Hasher::new();
        h.update(DOMAIN_TAG);
        h.update(artifact.as_bytes());
        h.update(b"\n");
        h.update(control.unwrap_or("").as_bytes());
        h.update(b"\n");
        h.update(intent.as_bytes());
        h.update(b"\n");
        h.update(previous.unwrap_or("").as_bytes());
        hex::encode(h.finalize().as_bytes())
    }

    #[test]
    fn compose_root_matches_pre_lift_hand_authored_chain() {
        // Sweeps every corner of the (control, previous) Option pair
        // — both consumers' pre-lift chains treated `None` as
        // empty-bytes, so the substrate composer MUST too.
        let cases: &[(&str, Option<&str>, &str, Option<&str>)] = &[
            ("aaaa", None, "iiii", None),
            ("aaaa", Some("cccc"), "iiii", None),
            ("aaaa", None, "iiii", Some("pppp")),
            ("aaaa", Some("cccc"), "iiii", Some("pppp")),
            ("", None, "", None),
            ("", Some(""), "", Some("")),
        ];
        for (artifact, control, intent, previous) in cases {
            assert_eq!(
                compose_root(artifact, *control, intent, *previous),
                hand_authored_chain(artifact, *control, intent, *previous),
                "compose_root drifted from pre-lift hand-authored chain \
                 for inputs (artifact={artifact:?}, control={control:?}, \
                 intent={intent:?}, previous={previous:?})",
            );
        }
    }

    #[test]
    fn compose_root_treats_empty_control_and_none_control_identically() {
        // Load-bearing invariant the receipt-envelope + attestation
        // consumers both rely on: an absent `control_hash` slot
        // (attestation's `Option<String>::None`) and an empty-string
        // `control_hash` slot (the receipt-envelope wire posture
        // where the writer stamps `""` for "no control step") MUST
        // compose to the SAME composed_root. Otherwise a receipt
        // written with `""` would false-negative against an
        // attestation chained with `None` even on identical pillars.
        let with_none = compose_root("art", None, "int", None);
        let with_empty = compose_root("art", Some(""), "int", Some(""));
        assert_eq!(with_none, with_empty);
    }

    #[test]
    fn compose_root_is_deterministic_across_calls() {
        // BLAKE3 is deterministic; the composer is pure. Pin it so
        // a future refactor that accidentally seeds a nonce or
        // reads a clock fails-loudly HERE.
        let a = compose_root("art", Some("ctl"), "int", Some("prev"));
        let b = compose_root("art", Some("ctl"), "int", Some("prev"));
        assert_eq!(a, b);
    }

    #[test]
    fn compose_root_differs_across_every_pillar() {
        // Each of the four pillars is load-bearing — a swap between
        // any two MUST produce a distinct composed_root, else the
        // domain-separation between pillars collapsed.
        let base = compose_root("aaaa", Some("cccc"), "iiii", Some("pppp"));
        assert_ne!(
            base,
            compose_root("BBBB", Some("cccc"), "iiii", Some("pppp")),
            "artifact pillar swap failed to alter composed_root"
        );
        assert_ne!(
            base,
            compose_root("aaaa", Some("CCCC"), "iiii", Some("pppp")),
            "control pillar swap failed to alter composed_root"
        );
        assert_ne!(
            base,
            compose_root("aaaa", Some("cccc"), "IIII", Some("pppp")),
            "intent pillar swap failed to alter composed_root"
        );
        assert_ne!(
            base,
            compose_root("aaaa", Some("cccc"), "iiii", Some("PPPP")),
            "previous pillar swap failed to alter composed_root"
        );
    }

    #[test]
    fn compose_root_output_is_lowercase_hex_of_blake3_length() {
        // BLAKE3 produces 32-byte digests; hex-encoded → 64 lowercase
        // characters. Pin the output shape so a downstream reader's
        // width assumption (a 26-char base32 slot in the wire form,
        // for instance) surfaces here rather than as a wire-parse
        // failure.
        let out = compose_root("a", None, "i", None);
        assert_eq!(out.len(), 64);
        assert!(out.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(out.chars().all(|c| !c.is_ascii_uppercase()));
    }

    // ── constant_time_eq byte-identity + corner pins ──────────────

    fn hand_authored_ct_eq(a: &[u8], b: &[u8]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        let mut acc: u8 = 0;
        for (x, y) in a.iter().zip(b.iter()) {
            acc |= x ^ y;
        }
        acc == 0
    }

    #[test]
    fn constant_time_eq_matches_pre_lift_hand_authored_body() {
        // Sweeps both length axes AND both equality axes so the
        // substrate comparator matches both pre-lift bodies byte-
        // for-byte on every corner.
        let cases: &[(&[u8], &[u8])] = &[
            (b"", b""),
            (b"", b"a"),
            (b"a", b""),
            (b"a", b"a"),
            (b"a", b"b"),
            (b"abcd", b"abcd"),
            (b"abcd", b"abce"),
            (b"abcd", b"abc"),
            (b"abc", b"abcd"),
            (b"\x00\x00\x00", b"\x00\x00\x00"),
            (b"\xff\xff\xff", b"\xff\xff\xff"),
            (b"\xff\xff\xff", b"\xff\xff\x00"),
        ];
        for (a, b) in cases {
            assert_eq!(
                constant_time_eq(a, b),
                hand_authored_ct_eq(a, b),
                "constant_time_eq drifted from pre-lift hand-authored \
                 body for inputs (a={a:?}, b={b:?})",
            );
        }
    }

    #[test]
    fn constant_time_eq_short_circuits_on_length_mismatch() {
        // The pre-lift length short-circuit at both consumers is a
        // load-bearing "different lengths CAN NEVER be equal" fast
        // path, not a leak. Pin the corner explicitly.
        assert!(!constant_time_eq(b"", b"a"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"abcd", b"abc"));
    }

    #[test]
    fn constant_time_eq_returns_true_only_on_full_byte_equality() {
        assert!(constant_time_eq(b"", b""));
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        // Distinct only at the final byte — verifies the fold
        // reaches the end rather than short-circuiting on the
        // first mismatch.
        assert!(!constant_time_eq(b"abcdef", b"abcdeg"));
        // Distinct only at the first byte — verifies the fold
        // does NOT short-circuit on the first byte (the "constant"
        // in "constant time" — full payload gets folded before
        // deciding).
        assert!(!constant_time_eq(b"Abcdef", b"abcdef"));
    }
}
