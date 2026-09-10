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

/// The canonical `"blake3:"` scheme prefix every three-pillar wire-form
/// BLAKE3 hash string in this crate opens with — the intra-crate ONE
/// substrate owner of the scheme literal on the READ (predicate) AND
/// WRITE (compose) axes.
///
/// ## Why the substrate lives here
///
/// Pre-lift the SAME `"blake3:"` bare literal was hand-authored across
/// ONE WRITE site ([`scheme_display_hash`]'s `format!("blake3:{h}")`
/// composition — the wrap primitive both [`pillar_hash`] and
/// [`compose_root`] route through) AND ~13 READ-side test predicates
/// (`att.<pillar>.starts_with("blake3:")` in `test_produce_attestation`,
/// `root.starts_with("blake3:")` in the `compose_root` scheme-prefix
/// pin, `pillar_hash(_).starts_with("blake3:")` × 2 in the pillar-hash
/// prefix pin, `scheme_display_hash(_).starts_with("blake3:")` × 2 in
/// the wrap-primitive prefix pin, `out.starts_with("blake3:")` × 2 in
/// the length pins, and 2 sites in `dag_executor::tests` guarding the
/// ATTEST-phase attestation-string composer). Every READ site walked
/// the SAME 1-link `.starts_with("blake3:")` shape, and every one was
/// re-authoring the same bare literal the WRITE composition emits.
///
/// Post-lift the scheme literal appears at ONE spelling here, and
/// every WRITE and READ site routes through it. A future scheme
/// change — a `b3:` shorthand, a version discriminator (`blake3-v2:`),
/// a per-fleet suffix — lands at ONE constant and both the wrap
/// primitive AND every downstream READ predicate inherit the shift
/// mechanically. Pre-lift such a rename would touch the write side
/// AND every hand-authored `.starts_with(...)` predicate; the two
/// sides could drift silently under review and only surface as a
/// produce/verify skew across the workspace's three-pillar consumers.
///
/// ## Sibling owner in `tatara-lisp`
///
/// [`tatara_lisp::hash::BLAKE3_SCHEME_PREFIX`] owns the SAME canonical
/// literal at the workspace layer as the substrate owner every
/// `tatara-lisp`-depending crate reaches for. That owner cannot fold
/// into this one because `tatara-engine` does not depend on
/// `tatara-lisp` (an intentional dep-graph choice: the engine crate
/// carries the seven-driver executor + Raft/gossip planes, and the
/// Lisp reader has no place in that graph). The two owners partition
/// the same scheme-literal surface at the crate boundary; both hold
/// the same value + shape, pinned at
/// [`tests::blake3_scheme_prefix_constant_value_is_the_canonical_seven_byte_ascii_literal`]
/// so a divergence between the two crate-layer owners' spellings
/// surfaces HERE rather than as silent cross-crate composed-root /
/// pillar-hash parse drift.
///
/// Theory anchor: THEORY.md §II.1 invariant 5 (composition preserves
/// proofs — the wire-form scheme literal at ONE substrate owner means
/// every WRITE-side wrap and every READ-side predicate agree
/// bytewise by construction). THEORY.md §V.3 (three-pillar
/// attestation — the canonical pillar-hash shape is `"blake3:{hex}"`;
/// this constant pins the scheme prefix half of that shape at the
/// intra-crate scheme-owner axis).
pub(crate) const BLAKE3_SCHEME_PREFIX: &str = "blake3:";

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
    scheme_display_hash(blake3::hash(bytes))
}

/// The `"blake3:"`-prefixed wire form of an already-computed
/// [`blake3::Hash`] handle — the intra-module ONE substrate owner of
/// the `format!("blake3:{}", <blake3::Hash>)` one-link `Display` wrap
/// [`pillar_hash`] (the one-shot `blake3::hash(bytes)` path) and
/// [`compose_root`] (the incremental `blake3::Hasher::new()` /
/// `.finalize()` path) each restated verbatim past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold.
///
/// ## Pre-lift consumers
///
/// * [`pillar_hash`] — the byte-buffer → `"blake3:{hex}"` pillar
///   projector (`format!("blake3:{}", blake3::hash(bytes))`).
/// * [`compose_root`] — the four-pillar Merkle composer's write tail
///   (`format!("blake3:{}", hasher.finalize())`), where the pillar
///   cascade folds the four typed slots into ONE `blake3::Hash` and
///   the wrap step attaches the canonical scheme prefix.
///
/// Both sites walked the SAME one-link chain — take a `blake3::Hash`
/// (from either the one-shot `blake3::hash(&[u8])` constructor or the
/// incremental `Hasher::finalize()` finisher) and prepend the module's
/// canonical `"blake3:"` scheme literal via the hash's `Display` impl
/// — differing only in the CALLING site's route to the hash value.
/// Post-lift the wrap step lives at ONE substrate owner and each
/// callsite passes its `blake3::Hash` receiver through by value; the
/// `"blake3:"` scheme literal appears at ONE spelling here.
///
/// ## Sibling axis
///
/// Sibling on the (one-shot, incremental) `blake3::Hash` origin axis:
/// [`pillar_hash`] owns the (bytes, `blake3::hash`) → wire-form arm,
/// where the `blake3::hash` constructor is the one-shot digest of a
/// byte buffer; [`compose_root`] owns the (Hasher, `.finalize()`) →
/// wire-form arm, where the incremental `Hasher::update(..)` cascade
/// feeds the digest across four typed slots before finishing. Both
/// arms produce a `blake3::Hash` value and route through this ONE
/// wrap primitive for the scheme-prefix write step — the (origin,
/// wrap) partition mirrors [`hex_blake3_of_json`] (bare hex,
/// `serde_json → BLAKE3` origin, no wrap) vs
/// [`blake3_scheme_display`](../../../../tatara_lisp/hash/fn.blake3_scheme_display.html)
/// (the workspace-wide wrap primitive on the `Display`-hex-string
/// input axis in `tatara-lisp`). This owner is the intra-module peer
/// on the `blake3::Hash` input axis; the workspace-wide peer stays
/// out of reach because `tatara-engine` does not depend on
/// `tatara-lisp` (an intentional dep-graph choice: the engine crate
/// carries the seven-driver executor + Raft/gossip planes, and the
/// Lisp reader has no place in that graph).
///
/// ## Byte-shape parity
///
/// `blake3::Hash: Display` encodes as exactly 64 lowercase ASCII hex
/// chars ([`blake3::Hash`]'s documented `Display` impl); the `format!`
/// composition thus produces `"blake3:" (7 chars) + 64 hex chars = 71
/// chars`. Byte-shape parity with the pre-lift hand-authored one-link
/// spelling is pinned at
/// [`tests::scheme_display_hash_matches_pre_lift_format_scheme_chain_bytewise`]
/// so a substrate-side canonicalization the pre-lift chain does NOT
/// apply (a hex-case flip, a scheme rename, a length-tag insertion, a
/// reversed prefix/hex order) surfaces at THIS pin rather than as
/// silent produce/verify skew across every three-pillar consumer.
///
/// ## `#[must_use]`
///
/// Every consumer stores the returned wire form into an attestation
/// pillar slot (`artifact_hash` / `control_hash` / `intent_hash`) or
/// into a `composed_root` slot. Dropping the return means the wrap
/// was performed for no observable reason; the attribute surfaces
/// that as a warning at every call site.
///
/// Theory anchor: THEORY.md §V.3 (three-pillar attestation — the
/// canonical pillar-hash shape is `"blake3:{hex}"`; this primitive
/// owns the wrap-only half of that shape on the `blake3::Hash` input
/// axis, sibling to [`pillar_hash`] on the byte-buffer input axis
/// and to [`compose_root`] on the incremental-hasher input axis).
/// THEORY.md §VI.1 (generation over composition — the one-link
/// `format!("blake3:{}", <blake3::Hash>)` chain recurred at TWO
/// production emit sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// trigger, and is lifted to ONE substrate owner here). THEORY.md
/// §II.1 invariant 5 (composition preserves proofs — routing both
/// producer sites through the SAME wrap primitive means a future
/// scheme change lands at ONE substrate site and every downstream
/// pillar / composed-root emit inherits the shift by construction).
#[must_use]
fn scheme_display_hash(h: blake3::Hash) -> String {
    format!("{BLAKE3_SCHEME_PREFIX}{h}")
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
    scheme_display_hash(hasher.finalize())
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
        assert!(att.artifact_hash.starts_with(BLAKE3_SCHEME_PREFIX));
        assert!(att
            .control_hash
            .as_ref()
            .unwrap()
            .starts_with(BLAKE3_SCHEME_PREFIX));
        assert!(att.intent_hash.starts_with(BLAKE3_SCHEME_PREFIX));
        assert!(att.composed_root.starts_with(BLAKE3_SCHEME_PREFIX));
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
            root.starts_with(BLAKE3_SCHEME_PREFIX),
            "composed root must carry the `{BLAKE3_SCHEME_PREFIX}` scheme prefix, got {root:?}",
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
            pillar_hash(b"any").starts_with(BLAKE3_SCHEME_PREFIX),
            "pillar_hash output must carry the `{BLAKE3_SCHEME_PREFIX}` scheme prefix",
        );
        assert!(
            pillar_hash(b"").starts_with(BLAKE3_SCHEME_PREFIX),
            "pillar_hash output must carry the `{BLAKE3_SCHEME_PREFIX}` scheme prefix on the empty input too",
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
        assert_eq!(
            out.len(),
            BLAKE3_SCHEME_PREFIX.len() + 64,
            "pillar_hash length must be BLAKE3_SCHEME_PREFIX.len() + 64",
        );
        assert!(out.starts_with(BLAKE3_SCHEME_PREFIX));
        let hex = &out[BLAKE3_SCHEME_PREFIX.len()..];
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

    // ─── scheme_display_hash substrate pins ────────────────────────
    //
    // Fail-before-pass-after granularity: the `scheme_display_hash`
    // free function did not exist before this commit, so each test
    // below fails to compile pre-lift. Post-lift they collectively
    // pin the `blake3::Hash → "blake3:{hex}"` wrap-only shape at ONE
    // intra-module substrate owner — the primitive `pillar_hash` (the
    // one-shot `blake3::hash(bytes)` origin) AND `compose_root` (the
    // incremental `Hasher::finalize()` origin) both route through.
    // A regression that drifted the `"blake3:"` scheme prefix (a `b3:`
    // shorthand, an uppercase `BLAKE3:` variant), reversed the prefix
    // / hex order, swapped the `blake3::Hash: Display` receiver
    // spelling (`.to_hex()` bytes, `hex::encode(<hash>.as_bytes())`),
    // or dropped the wrap entirely surfaces HERE rather than as
    // silent produce/verify skew across every three-pillar consumer.

    #[test]
    fn scheme_display_hash_matches_pre_lift_format_scheme_chain_bytewise() {
        // Byte-identical parity with the pre-lift `format!("blake3:{}",
        // <blake3::Hash>)` spelling both production callers walked.
        // Sweeps representative `blake3::Hash` origins so the wrap
        // composes byte-identically across BOTH the one-shot
        // `blake3::hash(bytes)` corner (matches `pillar_hash`'s
        // pre-lift shape) AND the incremental `Hasher::finalize()`
        // corner (matches `compose_root`'s pre-lift shape). A
        // regression at the wrap primitive that broke byte identity
        // with the pre-lift shape at either corner surfaces HERE
        // rather than as silent attestation-chain drift downstream.
        for buf in [
            b"" as &[u8],
            b"x",
            b"artifact-payload",
            b"{\"kind\":\"tatara.export\"}",
            &[0u8; 128],
            &[0xFFu8; 256],
        ] {
            // (a) One-shot origin — matches `pillar_hash`'s pre-lift
            // shape verbatim.
            let one_shot = blake3::hash(buf);
            assert_eq!(
                scheme_display_hash(one_shot),
                format!("blake3:{one_shot}"),
                "scheme_display_hash drifted from pre-lift `format!(\"blake3:{{}}\", blake3::hash(_))` for buf.len()={}",
                buf.len(),
            );

            // (b) Incremental origin — matches `compose_root`'s
            // pre-lift shape verbatim.
            let incremental = {
                let mut hasher = blake3::Hasher::new();
                hasher.update(buf);
                hasher.finalize()
            };
            assert_eq!(
                scheme_display_hash(incremental),
                format!("blake3:{incremental}"),
                "scheme_display_hash drifted from pre-lift `format!(\"blake3:{{}}\", hasher.finalize())` for buf.len()={}",
                buf.len(),
            );
        }
    }

    #[test]
    fn scheme_display_hash_output_carries_blake3_scheme_prefix() {
        // Wire-format pin: every wrap output MUST begin with the
        // `"blake3:"` scheme prefix. A regression that dropped the
        // prefix (writing bare hex) or drifted the spelling (`b3:`,
        // `BLAKE3:`, `blake3=`) would silently break downstream
        // consumers whose regex or prefix-strip step expects the
        // exact `"blake3:"` scheme.
        assert!(
            scheme_display_hash(blake3::hash(b"any")).starts_with(BLAKE3_SCHEME_PREFIX),
            "scheme_display_hash output must carry the `{BLAKE3_SCHEME_PREFIX}` scheme prefix",
        );
        assert!(
            scheme_display_hash(blake3::hash(b"")).starts_with(BLAKE3_SCHEME_PREFIX),
            "scheme_display_hash output must carry the `{BLAKE3_SCHEME_PREFIX}` scheme prefix on the empty input too",
        );
    }

    #[test]
    fn scheme_display_hash_output_is_scheme_plus_64_lowercase_hex_chars() {
        // Shape pin: the `blake3::Hash: Display` impl encodes as
        // lowercase hex (64 chars for BLAKE3's 32-byte digest); the
        // composed output is thus `"blake3:" (7 chars) + 64 hex chars
        // = 71 chars`. Pin the invariant so a downstream reader's
        // width assumption (a fixed-width slot, a regex `^blake3:
        // [0-9a-f]{64}$`) surfaces here rather than as a parse
        // failure downstream.
        let out = scheme_display_hash(blake3::hash(b"pillar-input"));
        assert_eq!(
            out.len(),
            BLAKE3_SCHEME_PREFIX.len() + 64,
            "scheme_display_hash length must be BLAKE3_SCHEME_PREFIX.len() + 64",
        );
        assert!(out.starts_with(BLAKE3_SCHEME_PREFIX));
        let hex_tail = &out[BLAKE3_SCHEME_PREFIX.len()..];
        assert_eq!(hex_tail.len(), 64);
        assert!(
            hex_tail
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "scheme_display_hash hex tail must be lowercase [0-9a-f]: {hex_tail:?}",
        );
    }

    #[test]
    fn scheme_display_hash_deterministic_for_same_hash_receiver() {
        // Same `blake3::Hash` → same wrapped output (the wrap is a
        // pure `format!` composition). A regression that mixed
        // nondeterminism in (a wall-clock read, a random seed, a
        // per-fleet suffix on the wrap) would surface HERE rather
        // than as flaky attestation output.
        let h = blake3::hash(b"receiver");
        assert_eq!(scheme_display_hash(h), scheme_display_hash(h));
    }

    #[test]
    fn scheme_display_hash_partitions_by_hash_receiver_value() {
        // Distinct `blake3::Hash` receivers → distinct wrapped
        // outputs. The wrap primitive does not collapse or normalize
        // the receiver — every distinct hash rides through the
        // scheme prefix untouched. Pins the invariant the pre-lift
        // `blake3::Hash: Display` cascade produced (no aliasing, no
        // truncation, no hex-layer collision).
        let a = blake3::hash(b"a");
        let b = blake3::hash(b"b");
        assert_ne!(scheme_display_hash(a), scheme_display_hash(b));
    }

    #[test]
    fn pillar_hash_and_compose_root_route_through_scheme_display_hash_by_construction() {
        // Cross-consumer coherence: BOTH production sites now share
        // ONE wrap primitive on the `blake3::Hash` input axis. Pin
        // the invariant by feeding the SAME byte payload through
        // (a) `pillar_hash(bytes)` and (b) the substrate wrap applied
        // directly to `blake3::hash(bytes)` — byte-identical output.
        // Then pin the `compose_root` write-tail: feed the pillars
        // through `compose_root` and re-derive the expected wire form
        // by running the same 4-step cascade + `scheme_display_hash`
        // on the result. Both must agree. A regression that
        // specialized ONE consumer's wrap step (a per-pillar salt at
        // `pillar_hash`, a per-cascade version discriminator at
        // `compose_root`) surfaces HERE rather than as silent
        // produce/verify drift across every three-pillar consumer.
        let payload = b"pillar-payload";

        // (a) pillar_hash routes through scheme_display_hash.
        assert_eq!(
            pillar_hash(payload),
            scheme_display_hash(blake3::hash(payload)),
            "pillar_hash must route through scheme_display_hash",
        );

        // (b) compose_root's write tail routes through
        // scheme_display_hash — re-derive the expected wire form
        // through the same cascade + wrap step and confirm byte-
        // identical parity with what compose_root emitted.
        let via_compose = compose_root("blake3:A", Some("blake3:C"), "blake3:B", Some("blake3:D"));
        let via_re_derive = {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"blake3:A");
            hasher.update(b"blake3:C");
            hasher.update(b"blake3:B");
            hasher.update(b"blake3:D");
            scheme_display_hash(hasher.finalize())
        };
        assert_eq!(
            via_compose, via_re_derive,
            "compose_root's write tail must route through scheme_display_hash",
        );
    }

    // ─── BLAKE3_SCHEME_PREFIX substrate pins ────────────────────────
    //
    // Fail-before-pass-after granularity: the `BLAKE3_SCHEME_PREFIX`
    // constant did not exist before this commit, so each test below
    // fails to compile pre-lift. Post-lift they collectively pin the
    // scheme literal at ONE intra-crate substrate owner — every
    // WRITE-side wrap (`scheme_display_hash`) AND every READ-side
    // predicate (`att.starts_with(BLAKE3_SCHEME_PREFIX)` at ~13 test
    // sites in this file + 2 in `dag_executor::tests`) routes through
    // the SAME constant, so a future scheme rename (a `b3:`
    // shorthand, a version discriminator, a per-fleet suffix) lands
    // at ONE substrate literal and both write + read agree bytewise
    // by construction.

    #[test]
    fn blake3_scheme_prefix_constant_value_is_the_canonical_seven_byte_ascii_literal() {
        // Constant-value pin: the intra-crate scheme prefix IS the
        // seven-byte ASCII literal `"blake3:"`. Pins the shape so a
        // regression that widened it (a length tag, a version
        // discriminator suffix), narrowed it (a `b3:` shorthand), or
        // drifted the case (`BLAKE3:`) surfaces HERE and every
        // downstream READ / WRITE consumer sees the change through
        // the ONE substrate owner. Sibling to
        // `tatara_lisp::hash::tests::blake3_scheme_prefix_is_the_canonical_seven_byte_ascii_literal`
        // at the workspace-layer scheme owner — the two crate-layer
        // owners MUST hold the same value + shape, since a cross-crate
        // consumer (a `sekiban` admission webhook, a `kensa`
        // compliance checker, a HeartbeatChain scan tool) reading an
        // attestation emitted by this crate parses the scheme prefix
        // through whichever constant its own crate reaches for. A
        // divergence between the two crate-layer spellings surfaces
        // HERE rather than as silent cross-crate composed-root /
        // pillar-hash parse drift.
        assert_eq!(BLAKE3_SCHEME_PREFIX, "blake3:");
        assert_eq!(BLAKE3_SCHEME_PREFIX.len(), 7);
        assert!(BLAKE3_SCHEME_PREFIX.is_ascii());
    }

    #[test]
    fn scheme_display_hash_write_and_read_side_predicates_share_scheme_constant() {
        // Cross-consumer coherence: the WRITE-side wrap primitive
        // `scheme_display_hash` AND every READ-side
        // `.starts_with(BLAKE3_SCHEME_PREFIX)` predicate route through
        // the SAME canonical scheme literal. A rename of the constant's
        // VALUE (`"blake3:"` → `"b3:"`) would land the WRITE output on
        // the new prefix AND every READ predicate would accept the new
        // prefix as its scheme-present floor, keeping both sides in
        // lockstep at ONE substrate owner. Pins the invariant the
        // two-way partition between the bare `blake3::Hash: Display`
        // hex tail (no prefix) and the wrapped wire form
        // (prefix + hex) rides. Sibling of the tatara-lisp pin
        // `blake3_scheme_display_wrap_and_read_side_pin_share_scheme_constant`
        // on the workspace-layer scheme owner; both crate-layer owners
        // pin the same WRITE/READ coherence invariant.
        let out = scheme_display_hash(blake3::hash(b"any"));
        assert!(
            out.starts_with(BLAKE3_SCHEME_PREFIX),
            "WRITE-side wrap must carry the substrate's canonical scheme prefix",
        );
        // Length check via the constant length rather than a magic 7 —
        // pins that the wire-shape budget (prefix bytes + 64 hex chars)
        // stays in lockstep with the constant's length.
        assert_eq!(out.len(), BLAKE3_SCHEME_PREFIX.len() + 64);
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
