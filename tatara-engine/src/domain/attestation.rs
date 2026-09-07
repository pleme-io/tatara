//! Convergence attestation — binds tameshi CertificationArtifact to
//! convergence boundary phases.
//!
//! Every convergence point's Attest phase produces a three-pillar binding:
//!   artifact_hash  = blake3(convergence function output)
//!   control_hash   = blake3(compliance verification result)
//!   intent_hash    = blake3(Nix desired state)
//!
//! This module provides the attestation logic that the DagExecutor calls
//! during the Attest boundary phase.

use serde::{Deserialize, Serialize};

/// Substrate primitive over `&[u8]` — the ONE substrate owner of the
/// `format!("blake3:{}", blake3::hash(<bytes>))` two-link chain every
/// three-pillar producer restated by hand at the "cast an input
/// payload into its wire-form BLAKE3 hash string" boundary.
///
/// Sibling on the composed-hash axis to [`compose_root`] below: this
/// primitive owns the PER-PILLAR shape (`&[u8] → "blake3:{hex}"`);
/// `compose_root` owns the FOUR-PILLAR shape (already-hashed pillar
/// strings → composed root string). Both carry the `"blake3:"` scheme
/// prefix as a private literal so a future scheme change (a length
/// tag, a version discriminator, a per-fleet suffix) lands at these
/// two substrate owners rather than at the pre-lift hand-authored
/// sites listed below.
///
/// Pre-lift the two-link chain was hand-authored at FOUR production
/// emit sites across `tatara-engine` past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold:
///
/// * [`ConvergenceAttestation::produce`] × 3 — the three-pillar seed
///   family (`artifact_hash`, `control_hash` inside a `.map` over the
///   optional-slot input, `intent_hash`), each restating the same
///   `format!("blake3:{}", blake3::hash(<slot>))` shape verbatim.
/// * `dag_executor::execute_boundary` × 1 — the ATTEST-phase
///   attestation-string composer, restating the same shape over the
///   `attestation_data.as_bytes()` payload that carries the point's
///   name + input-attestation + postcondition count.
///
/// All FOUR pre-lift sites restated the SAME two-link chain verbatim,
/// differing only in the `&[u8]` payload each callsite fed in and (at
/// site #2) the `.map` wrap for the optional-slot input. A regression
/// that drifted the scheme prefix at ONE site (a `b3:` shorthand, an
/// uppercase `BLAKE3:` variant, an accidental spelling drift under a
/// future refactor), swapped the `blake3::hash` receiver spelling
/// (`.to_hex()`, `.as_bytes()` + `hex::encode`), or reversed the
/// prefix / hex order would silently break parity between the produce
/// path (`ConvergenceAttestation::produce`) and the ATTEST-phase
/// stamp in the executor — surfacing as verify failures on some
/// nodes and passes on others.
///
/// Post-lift each callsite reads `pillar_hash(<slot>)` and the
/// scheme-tagged pillar-hash shape lives at ONE substrate owner here.
///
/// ### Byte-shape parity
///
/// The `format!("blake3:{}", blake3::hash(bytes))` spelling produces
/// `"blake3:" + 64-lowercase-hex-chars` — the `blake3::Hash: Display`
/// impl encodes as lowercase hex. Byte-shape parity with the pre-lift
/// spelling is pinned at
/// [`tests::pillar_hash_matches_pre_lift_format_scheme_hex_spelling_bytewise`]
/// so a substrate-side canonicalization the pre-lift chain does NOT
/// apply (a case-flip on the hex, a scheme rename, a length-tag
/// insertion) surfaces at THIS pin rather than as silent
/// produce/verify skew across every three-pillar consumer.
///
/// ### `#[must_use]`
///
/// Every consumer stores the returned hex into a pillar slot
/// (`artifact_hash` / `control_hash` / `intent_hash`) or into the
/// executor's boundary-attestation output. Dropping the return means
/// the hash was computed for no observable reason — the attribute
/// surfaces that as a warning at every call site.
///
/// Theory anchor: THEORY.md §V.3 (three-pillar attestation — the
/// canonical pillar-hash shape is `"blake3:{hex}"`; this substrate
/// pins the shape at ONE owner). THEORY.md §VI.1 (generation over
/// composition — the two-link chain recurred at FOUR hand-authored
/// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and is
/// lifted to ONE substrate owner here). THEORY.md §II.1 invariant 5
/// (composition preserves proofs — the pin below binds the primitive
/// byte-identically to the pre-lift spelling so a regression at ONE
/// substrate function surfaces at ONE pin rather than as silent
/// forgery-adjacent skew across every downstream consumer).
#[must_use]
pub(crate) fn pillar_hash(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes))
}

/// A convergence attestation — the three-pillar CertificationArtifact
/// produced by each convergence point's boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvergenceAttestation {
    /// Blake3 hash of the convergence function output state.
    pub artifact_hash: String,
    /// Blake3 hash of compliance verification results (if any).
    pub control_hash: Option<String>,
    /// Blake3 hash of the Nix-declared desired state.
    pub intent_hash: String,
    /// Composed root = blake3(artifact || control || intent).
    pub composed_root: String,
    /// Generation counter (monotonic per re-convergence).
    pub generation: u64,
    /// Previous generation's composed_root (append-only chain).
    pub previous_root: Option<String>,
}

/// Compose the three-pillar composed root from the four typed pillar
/// slots — artifact + optional control + intent + optional previous —
/// the ONE substrate owner of the `blake3::Hasher::new()` +
/// `hasher.update(<pillar>.as_bytes())` cascade + optional-arm
/// short-circuits + `format!("blake3:{}", hasher.finalize())` write
/// tail that `ConvergenceAttestation::produce` (write side) and
/// `ConvergenceAttestation::verify` (read side) restated verbatim past
/// the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold.
///
/// Pre-lift each site opened its own `blake3::Hasher`, threaded the
/// same 5-statement cascade (artifact → optional control → intent →
/// optional previous → format!) — differing only in whether it read
/// from `self` (verify) or from freshly-computed locals (produce).
/// A drift in ONE cascade — an accidental reorder of the pillars, a
/// dropped optional-arm guard, a swapped `"blake3:"` prefix — would
/// silently break the produce/verify round-trip: attestations emitted
/// by one node would fail verification at another, or (worse) verify
/// under the wrong composition and admit forged attestations. The
/// invariant "produce and verify hash the SAME pillars in the SAME
/// order under the SAME wrap" now holds by construction rather than
/// by two hand-authored copies staying in sync under review.
///
/// Sibling on the composed-hash axis to
/// [`tatara-process::hash::hex_blake3`] (the flat 2-link
/// `hex::encode(blake3::hash(bytes))` write) and to
/// [`tatara-process::identity::content_hash`] (the identity-axis
/// canonical-form BLAKE3 → base32 projector): each closes ONE
/// hash-composition shape at ONE substrate owner. This owner closes
/// the three-pillar Merkle-with-optional-slots composition on
/// [`ConvergenceAttestation`].
///
/// The `"blake3:"` scheme prefix stays a private literal here — the
/// pre-lift pattern paired the prefix WITH the composition, so lifting
/// it separately would leak the wire format across two owners. A
/// future scheme change (a length tag, a version discriminator, a
/// per-fleet suffix) lands at this ONE call.
///
/// Theory anchor: THEORY.md §V.3 (three-pillar attestation — the
/// canonical `artifact ⊕ control ⊕ intent → BLAKE3 Merkle` composition
/// is defined here). THEORY.md §II.1 invariant 5 (composition
/// preserves proofs — the produce/verify byte-identity invariant now
/// holds by construction, pinned at
/// [`tests::compose_root_produce_and_verify_use_the_same_composition_by_construction`]).
#[must_use]
fn compose_root(
    artifact_hash: &str,
    control_hash: Option<&str>,
    intent_hash: &str,
    previous_root: Option<&str>,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(artifact_hash.as_bytes());
    if let Some(ch) = control_hash {
        hasher.update(ch.as_bytes());
    }
    hasher.update(intent_hash.as_bytes());
    if let Some(prev) = previous_root {
        hasher.update(prev.as_bytes());
    }
    format!("blake3:{}", hasher.finalize())
}

impl ConvergenceAttestation {
    /// Produce a new attestation from the three pillars.
    pub fn produce(
        artifact_data: &[u8],
        control_data: Option<&[u8]>,
        intent_data: &[u8],
        generation: u64,
        previous_root: Option<String>,
    ) -> Self {
        let artifact_hash = pillar_hash(artifact_data);
        let control_hash = control_data.map(pillar_hash);
        let intent_hash = pillar_hash(intent_data);

        let composed_root = compose_root(
            &artifact_hash,
            control_hash.as_deref(),
            &intent_hash,
            previous_root.as_deref(),
        );

        Self {
            artifact_hash,
            control_hash,
            intent_hash,
            composed_root,
            generation,
            previous_root,
        }
    }

    /// Verify the composed root is correct given the three pillars.
    pub fn verify(&self) -> bool {
        let expected = compose_root(
            &self.artifact_hash,
            self.control_hash.as_deref(),
            &self.intent_hash,
            self.previous_root.as_deref(),
        );
        self.composed_root == expected
    }
}

/// Compliance verification result that feeds into the control_hash pillar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceResult {
    /// Framework that was verified.
    pub framework: String,
    /// Controls that were checked.
    pub controls_checked: Vec<String>,
    /// Controls that passed.
    pub controls_passed: Vec<String>,
    /// Controls that failed.
    pub controls_failed: Vec<String>,
    /// Whether all controls passed.
    pub all_passed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_produce_attestation() {
        let att = ConvergenceAttestation::produce(
            b"workload running",
            Some(b"nist ac-6 passed"),
            b"desired: { replicas: 3 }",
            1,
            None,
        );
        assert!(att.artifact_hash.starts_with("blake3:"));
        assert!(att.control_hash.as_ref().unwrap().starts_with("blake3:"));
        assert!(att.intent_hash.starts_with("blake3:"));
        assert!(att.composed_root.starts_with("blake3:"));
        assert_eq!(att.generation, 1);
    }

    #[test]
    fn test_verify_attestation() {
        let att =
            ConvergenceAttestation::produce(b"artifact", Some(b"controls"), b"intent", 0, None);
        assert!(att.verify());
    }

    #[test]
    fn test_tampered_attestation_fails_verify() {
        let mut att =
            ConvergenceAttestation::produce(b"artifact", Some(b"controls"), b"intent", 0, None);
        att.artifact_hash = "blake3:tampered".into();
        assert!(!att.verify());
    }

    #[test]
    fn test_generational_chain() {
        let gen0 = ConvergenceAttestation::produce(b"v1", None, b"intent", 0, None);
        let gen1 = ConvergenceAttestation::produce(
            b"v2",
            None,
            b"intent",
            1,
            Some(gen0.composed_root.clone()),
        );
        assert!(gen1.verify());
        assert_eq!(
            gen1.previous_root.as_deref(),
            Some(gen0.composed_root.as_str())
        );
        assert_ne!(gen0.composed_root, gen1.composed_root);
    }

    #[test]
    fn test_no_compliance() {
        let att = ConvergenceAttestation::produce(b"artifact", None, b"intent", 0, None);
        assert!(att.control_hash.is_none());
        assert!(att.verify());
    }

    #[test]
    fn test_deterministic() {
        let a = ConvergenceAttestation::produce(b"x", Some(b"y"), b"z", 0, None);
        let b = ConvergenceAttestation::produce(b"x", Some(b"y"), b"z", 0, None);
        assert_eq!(a.composed_root, b.composed_root);
    }

    // ─── compose_root substrate pins ─────────────────────────────────
    //
    // Fail-before-pass-after granularity: the `compose_root` free function
    // did not exist before this commit, so each test below fails to compile
    // pre-lift. Post-lift they collectively pin the three-pillar
    // composition at ONE substrate owner — a regression that swapped the
    // pillar order (artifact ↔ intent — would silently break the
    // produce/verify round-trip on every cross-node attestation stream),
    // dropped an optional-slot guard (e.g. always hashing an empty string
    // in the None arm — would collapse the None/Some("") distinction the
    // Merkle chain relies on), or drifted the `"blake3:"` scheme prefix
    // surfaces HERE rather than as a silent forgery-adjacent gap between
    // the two callers.

    #[test]
    fn compose_root_produce_and_verify_use_the_same_composition_by_construction() {
        // The load-bearing invariant: an attestation `produce()`-ed by
        // any node MUST `verify()` at any other node given identical
        // pillar inputs. Pre-lift this held because two hand-authored
        // 5-statement cascades stayed in sync under review; post-lift
        // it holds because both callers route through ONE substrate
        // primitive. Sweeps the four optional-slot corners (control:
        // absent/present × previous: absent/present) so a regression
        // that mishandled ONE optional arm surfaces on that arm's row.
        for (control, previous) in [
            (None, None),
            (Some(&b"controls-passed"[..]), None),
            (None, Some("blake3:prev".to_string())),
            (
                Some(&b"controls-passed"[..]),
                Some("blake3:prev".to_string()),
            ),
        ] {
            let att = ConvergenceAttestation::produce(
                b"artifact-payload",
                control,
                b"intent-payload",
                7,
                previous,
            );
            assert!(
                att.verify(),
                "attestation produced with (control={:?}, previous={:?}) must verify",
                att.control_hash,
                att.previous_root,
            );
        }
    }

    #[test]
    fn compose_root_deterministic_across_calls() {
        // Same-inputs → same-output: the composition is a pure function
        // of its four pillar slots. A regression that mixed nondeterminism
        // in (a wall-clock read, a random seed, a HashMap iteration
        // order) would surface here.
        let a = compose_root("blake3:art", Some("blake3:ctl"), "blake3:int", Some("prev"));
        let b = compose_root("blake3:art", Some("blake3:ctl"), "blake3:int", Some("prev"));
        assert_eq!(a, b);
    }

    #[test]
    fn compose_root_pillar_order_is_load_bearing_artifact_before_intent() {
        // Swapping artifact ↔ intent MUST produce a distinct root — the
        // composition is order-sensitive by design so `artifact_hash`
        // and `intent_hash` remain distinguishable pillars on the wire
        // even when both slots carry byte-identical hashes. A regression
        // that concatenated the pillars into a set-shaped digest
        // (sorted, hashed jointly) would silently break attestations
        // whose artifact/intent hashes collide.
        let ordered = compose_root("blake3:A", None, "blake3:B", None);
        let swapped = compose_root("blake3:B", None, "blake3:A", None);
        assert_ne!(
            ordered, swapped,
            "artifact-before-intent order is load-bearing on the composed root",
        );
    }

    #[test]
    fn compose_root_optional_slot_absence_differs_from_presence_when_bytes_nonempty() {
        // The `None` arm skips the `hasher.update(...)` call entirely;
        // the `Some(s)` arm calls `hasher.update(s.as_bytes())`. Pin
        // the distinction the callers rely on: `None` means "this
        // pillar is not part of the chain," `Some(nonempty)` means
        // "hash `s.as_bytes()` into the state at this pillar's slot."
        // A regression that unconditionally hashed a placeholder
        // sentinel in the `None` arm would surface here.
        let control_absent = compose_root("blake3:A", None, "blake3:B", None);
        let previous_absent = compose_root("blake3:A", Some("blake3:C"), "blake3:B", None);
        let both_present = compose_root("blake3:A", Some("blake3:C"), "blake3:B", Some("blake3:D"));
        assert_ne!(control_absent, previous_absent);
        assert_ne!(previous_absent, both_present);
        assert_ne!(control_absent, both_present);
    }

    #[test]
    fn compose_root_output_carries_blake3_scheme_prefix() {
        // Wire-format pin: every composed root MUST begin with the
        // `"blake3:"` scheme prefix, matching the three pillar hashes'
        // own scheme discipline (`blake3:{hex}`). A regression that
        // dropped the prefix (writing bare hex) or drifted the scheme
        // spelling (`b3:`, `BLAKE3:`) would silently break downstream
        // consumers (sekiban admission webhooks, kensa compliance
        // checkers, HeartbeatChain scan tools) whose regex or prefix-
        // strip step expects the exact `"blake3:"` scheme.
        let root = compose_root("blake3:A", None, "blake3:B", None);
        assert!(
            root.starts_with("blake3:"),
            "composed root must carry the `blake3:` scheme prefix, got {root:?}",
        );
    }

    #[test]
    fn compose_root_matches_pre_lift_hand_authored_composition_bytewise() {
        // Byte-shape parity pin: `compose_root(a, c, i, p)` MUST produce
        // the SAME `String` the pre-lift hand-authored 5-statement
        // cascade produced. Sweeps the four optional-slot corners so a
        // regression at the primitive that broke byte identity with
        // the pre-lift shape at ONE corner surfaces here rather than
        // as silent attestation-chain drift at every downstream
        // consumer.
        for (control, previous) in [
            (None, None),
            (Some("blake3:C"), None),
            (None, Some("blake3:P")),
            (Some("blake3:C"), Some("blake3:P")),
        ] {
            let via_primitive = compose_root("blake3:A", control, "blake3:I", previous);

            // Pre-lift 5-statement cascade — the exact bytes both
            // `produce()` and `verify()` open-coded before this commit.
            let via_pre_lift = {
                let mut hasher = blake3::Hasher::new();
                hasher.update(b"blake3:A");
                if let Some(ch) = control {
                    hasher.update(ch.as_bytes());
                }
                hasher.update(b"blake3:I");
                if let Some(pr) = previous {
                    hasher.update(pr.as_bytes());
                }
                format!("blake3:{}", hasher.finalize())
            };

            assert_eq!(via_primitive, via_pre_lift);
        }
    }

    // ─── pillar_hash substrate pins ────────────────────────────────
    //
    // Fail-before-pass-after granularity: the `pillar_hash` free
    // function did not exist before this commit, so each test below
    // fails to compile pre-lift. Post-lift they collectively pin the
    // scheme-tagged pillar-hash shape at ONE substrate owner — a
    // regression that drifted the `"blake3:"` scheme prefix, swapped
    // the `blake3::hash` receiver spelling (`.to_hex()`, `.as_bytes()`
    // + `hex::encode`), reversed the prefix / hex order, or leaked a
    // canonicalization the pre-lift `format!` chain does NOT apply
    // surfaces HERE rather than as silent produce/verify skew across
    // every three-pillar consumer.

    #[test]
    fn pillar_hash_matches_pre_lift_format_scheme_hex_spelling_bytewise() {
        // Byte-identical parity with the pre-lift `format!("blake3:{}",
        // blake3::hash(x))` spelling every three-pillar producer + the
        // dag_executor ATTEST-phase composer walked. Swept across
        // representative payload shapes (empty, short ASCII, JSON-like,
        // large binary) so a substrate-side canonicalization the
        // pre-lift chain does NOT apply (a hex-case flip, a `blake3:`
        // scheme rename, a length-tag insertion, a re-ordering of
        // prefix/hex) surfaces HERE rather than as silent forgery-
        // adjacent skew at every downstream consumer.
        for buf in [
            b"" as &[u8],
            b"x",
            b"artifact-payload",
            b"{\"kind\":\"tatara.export\"}",
            &[0u8; 128],
            &[0xFFu8; 256],
        ] {
            assert_eq!(
                pillar_hash(buf),
                format!("blake3:{}", blake3::hash(buf)),
                "pillar_hash drifted from pre-lift `format!(\"blake3:{{}}\", blake3::hash(_))` for buf.len()={}",
                buf.len(),
            );
        }
    }

    #[test]
    fn pillar_hash_output_carries_blake3_scheme_prefix() {
        // Wire-format pin: every pillar hash MUST begin with the
        // `"blake3:"` scheme prefix, matching the composed_root's own
        // scheme discipline and the pre-lift `format!` template. A
        // regression that dropped the prefix (writing bare hex) or
        // drifted the spelling (`b3:`, `BLAKE3:`, `blake3=`) would
        // silently break downstream consumers whose prefix-strip or
        // regex step expects the exact `"blake3:"` scheme.
        assert!(
            pillar_hash(b"any").starts_with("blake3:"),
            "pillar_hash output must carry the `blake3:` scheme prefix",
        );
        assert!(
            pillar_hash(b"").starts_with("blake3:"),
            "pillar_hash output must carry the `blake3:` scheme prefix on the empty input too",
        );
    }

    #[test]
    fn pillar_hash_output_is_scheme_plus_64_lowercase_hex_chars() {
        // Shape pin: the `blake3::Hash: Display` impl encodes as
        // lowercase hex (64 chars for BLAKE3's 32-byte digest); the
        // composed output is thus `"blake3:" (7 chars) + 64 hex chars
        // = 71 chars`. Pin the invariant so a downstream reader's
        // width assumption (a fixed-width slot, a regex `^blake3:
        // [0-9a-f]{64}$`) surfaces here rather than as a parse
        // failure downstream.
        let out = pillar_hash(b"pillar-input");
        assert_eq!(out.len(), 7 + 64, "pillar_hash length must be 7 + 64");
        assert!(out.starts_with("blake3:"));
        let hex = &out[7..];
        assert_eq!(hex.len(), 64);
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "pillar_hash hex tail must be lowercase [0-9a-f]: {hex:?}",
        );
    }

    #[test]
    fn pillar_hash_deterministic_and_distinct_inputs_distinct_outputs() {
        // Same input → same output (BLAKE3 + format! are both pure).
        // Distinct inputs → distinct outputs (no hex-layer collision,
        // no shared-prefix truncation, no substrate-side normalization
        // that collapses payloads).
        assert_eq!(pillar_hash(b"same"), pillar_hash(b"same"));
        assert_ne!(pillar_hash(b"a"), pillar_hash(b"b"));
    }

    #[test]
    fn pillar_hash_empty_input_matches_known_digest_with_scheme_prefix() {
        // Known BLAKE3 digest of the empty input, wearing the
        // `"blake3:"` scheme prefix. A rename of the underlying algo
        // (an accidental switch to sha2, a salt smuggled through the
        // Hasher::new constructor) or a drift in the scheme spelling
        // would land here rather than as silent attestation-chain
        // drift across every downstream consumer.
        assert_eq!(
            pillar_hash(b""),
            "blake3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
        );
    }

    #[test]
    fn produce_pillars_route_through_pillar_hash_by_construction() {
        // Cross-consumer coherence: an attestation `produce()`-ed
        // through `ConvergenceAttestation::produce` MUST expose
        // pillar hashes byte-identical to `pillar_hash` over the
        // same input payloads. Pre-lift this held because the four
        // sites open-coded the same `format!` template; post-lift it
        // holds because both callers route through ONE substrate
        // primitive. A regression that specialized `produce`'s
        // pillar-hash step (a per-pillar salt, a version prefix) or
        // reshaped `pillar_hash`'s output would surface HERE rather
        // than as silent produce/verify skew at every three-pillar
        // consumer.
        let att = ConvergenceAttestation::produce(
            b"artifact-payload",
            Some(b"control-payload"),
            b"intent-payload",
            0,
            None,
        );
        assert_eq!(att.artifact_hash, pillar_hash(b"artifact-payload"));
        assert_eq!(
            att.control_hash.as_deref(),
            Some(pillar_hash(b"control-payload").as_str())
        );
        assert_eq!(att.intent_hash, pillar_hash(b"intent-payload"));
    }

    #[test]
    fn test_compliance_result() {
        let result = ComplianceResult {
            framework: "nist-800-53".into(),
            controls_checked: vec!["AC-6".into(), "AU-2".into()],
            controls_passed: vec!["AC-6".into(), "AU-2".into()],
            controls_failed: vec![],
            all_passed: true,
        };
        assert!(result.all_passed);
        assert_eq!(result.controls_checked.len(), 2);
    }
}
