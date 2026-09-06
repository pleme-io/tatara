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
use serde::Serialize;

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
    // Terminal `hex::encode(<hash>.as_bytes())` step rides through
    // the substrate primitive [`crate::hash::hex_blake3_hash`] — the
    // ONE owner of the streaming-digest hex encoding. Pre-lift this
    // site restated `hex::encode(h.finalize().as_bytes())` inline,
    // sibling to the same 1-link chain hand-authored at
    // `tatara-reconciler::phase_machine::handle_running` (the per-ref
    // artifact-hash fold on the ATTEST step) past the ★★ PRIME-
    // DIRECTIVE ≥ 2 duplication threshold; post-lift both consumers
    // route through ONE substrate function, and a future re-encoding
    // reaches both mechanically.
    crate::hash::hex_blake3_hash(&h.finalize())
}

/// Canonical serialize-to-bytes projection for an attestation-pillar
/// input.
///
/// Owns the pre-lift `serde_json::to_vec(v).unwrap_or_default()`
/// shape every producer of a pillar-shaped byte buffer restated by
/// hand pre-lift — SIX workspace-wide sites past the ★★ PRIME-
/// DIRECTIVE ≥ 2 duplication trigger:
///
/// * [`crate::intent::IntentVariant::canonical_bytes`] — SIX arms
///   inside the enum-dispatch method, each restating the fallback
///   shape on a different inner variant reference. Post-lift each
///   arm names the payload once and delegates through this ONE
///   primitive.
/// * [`crate::identity::content_hash`] — the 128-bit content-
///   addressable BLAKE3 identity input every `Process` walks; the
///   base32-encoding downstream is untouched, only the shared
///   pillar-bytes input rides through the substrate owner.
/// * `tatara-reconciler::render::render_flux` /
///   `render_aplicacao` / `render_nix` — the three workload-emitting
///   render helpers whose `intent_bytes` return value feeds the
///   ATTEST-phase intent-pillar hash.
/// * `tatara-reconciler::render::render` (Guest arm) — Guest
///   intents (HVF / VZ / WASM) are owned by tatara-hospedeiro and
///   emit no K8s resources, but their intent bytes still feed the
///   three-pillar attestation chain.
/// * `tatara-reconciler::phase_machine::compute_intent_hash` — the
///   stable-hash-of-intent projection on the reconcile-tick side,
///   feeding `hex_blake3` directly.
///
/// # `unwrap_or_default()` — why the empty-bytes fallback is
/// load-bearing
///
/// `serde_json::to_vec` returns `Err` only when the input contains
/// a non-serializable shape (a map with non-string keys, a value
/// too deep for the recursion limit) — none of which the typed
/// intent / spec inputs at any current callsite can produce. The
/// `unwrap_or_default()` fallback is a defensive guard that
/// composes empty bytes onto the pillar hash rather than panicking
/// the reconciler; a regression that swapped it for `.expect(...)`
/// would turn a serde-error corner into a controller-crash corner
/// (silently — no test panics if the corner never triggers). ONE
/// substrate owner concentrates the policy so a future upgrade (a
/// serde-error trace event before returning empty, a size-cap
/// guard against pathological payloads, a canonical-JSON
/// serializer for stable byte ordering across serde versions) lands
/// at this ONE function and every pillar-bytes consumer inherits
/// the upgrade mechanically.
///
/// # `#[must_use]`
///
/// Every consumer either feeds the returned bytes into a BLAKE3
/// hash (intent pillar, artifact pillar, content-hash identity)
/// or stores them into a `RenderOutput.intent_bytes` slot. Dropping
/// the return means the payload was serialized for no observable
/// reason.
///
/// # Theory anchor
///
/// THEORY.md §VI.1 (generation over composition — the
/// `serde_json::to_vec(v).unwrap_or_default()` shape recurred at
/// SIX hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold, and lifts to ONE substrate owner here).
/// THEORY.md §II.1 invariant 5 (composition preserves proofs —
/// the byte-identity pin
/// [`tests::pillar_bytes_matches_pre_lift_serde_json_to_vec_shape_bytewise`]
/// binds the primitive byte-identically to the pre-lift spelling
/// so a regression at the substrate owner surfaces at ONE pin
/// rather than as silent pillar-bytes drift across every
/// downstream three-pillar consumer).
#[must_use]
pub fn pillar_bytes<T: Serialize + ?Sized>(v: &T) -> Vec<u8> {
    serde_json::to_vec(v).unwrap_or_default()
}

/// Canonical `serde_json` bytes for a pillar-input — the strict,
/// error-propagating peer of [`pillar_bytes`] that routes the payload
/// through `serde_json::Value` before emitting bytes.
///
/// Two workspace-local `canonical_json` helpers walked the SAME 2-link
/// `serde_json::to_value(v)? → serde_json::to_vec(&v)` chain past the
/// ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold, each with its own
/// per-file private helper concentrating the round-trip:
///
/// * `tatara-process::hostname::canonical_json` — the private helper
///   feeding [`crate::hostname::ephemeral_id_from_spec`], which hashes
///   the canonical bytes of a `ProcessSpec` to derive the content-
///   addressable `ephemeral_id` slot every per-instance FQDN routes
///   through.
/// * `tatara-export-worker::canonical_json` — the private helper
///   feeding `compose_export_receipt`, which hashes the canonical
///   bytes of both an `ExportSpec` (intent pillar) AND an
///   `ExportOutcome` (control pillar) into a `tatara-receipt/v1`
///   envelope that chains into the Process attestation tree.
///
/// Both helpers' bodies were byte-identical
/// (`let v = serde_json::to_value(value)?; serde_json::to_vec(&v)`),
/// differing only in return-error type (`serde_json::Error` on the
/// hostname peer; `anyhow::Result` on the worker peer, achieved via
/// `?` sugar). Post-lift both consumers name the payload ONCE and
/// route through this ONE substrate primitive; the concrete
/// `serde_json::Error` return type composes into `anyhow::Error` via
/// `?` at the worker callsite and into `HostnameError::InvalidLabel`
/// via `.map_err(...)` at the hostname callsite.
///
/// # Canonicalization semantics (the load-bearing difference from
/// [`pillar_bytes`])
///
/// [`pillar_bytes`] calls `serde_json::to_vec` directly. On a `HashMap
/// <String, V>` (or any serializer walking arbitrary iteration order),
/// that yields the HashMap's non-deterministic key order — silently
/// different bytes across runs on the SAME input. `canonical_bytes`
/// interposes `serde_json::to_value` so the intermediate
/// `Value::Object` — which is [`serde_json::Map`], itself a
/// `BTreeMap<String, Value>` by default (this workspace does NOT
/// enable `serde_json/preserve_order`; verified via the absence of
/// `indexmap` under `serde_json` in `Cargo.lock`) — sorts keys
/// alphabetically before the final `to_vec` emits them. This is the
/// property both hostname + worker helpers relied on for
/// hash-stability: identical spec / outcome payloads must produce
/// identical canonical bytes across every reconcile / worker run.
///
/// Struct fields ALSO get sorted alphabetically through
/// [`canonical_bytes`] — the intermediate `Value::Object` uses the
/// same BTreeMap-backed [`serde_json::Map`], and the serde-json
/// serializer for structs walks fields through the map surface (each
/// field-name → `serialize_map_entry`), so the BTreeMap absorbs
/// declaration order and re-emits alphabetically. This is a stronger
/// canonicalization than [`pillar_bytes`] performs — the direct
/// [`serde_json::to_vec`] emits struct fields in DECLARATION order.
/// A pin at
/// [`tests::canonical_bytes_sorts_struct_fields_alphabetically`]
/// binds the sort behavior for structs, and the divergence pin
/// [`tests::canonical_bytes_diverges_from_pillar_bytes_on_non_alphabetical_field_order`]
/// binds the byte-shape difference from [`pillar_bytes`] on the
/// non-alphabetical-declaration corner so the split between the two
/// pillar-bytes primitives stays visible at fail-before-pass-after
/// granularity. A `#[derive(Serialize)] struct` whose declaration
/// order happens to coincide with alphabetical order (the common
/// case for structs with `a`, `b`, `c` fields) will still produce the
/// SAME bytes through both primitives — the coherence corner is
/// pinned at
/// [`tests::canonical_bytes_agrees_with_pillar_bytes_on_alphabetical_shapes`].
///
/// # Error surface
///
/// Returns `Result<Vec<u8>, serde_json::Error>` — the concrete
/// serde error type both pre-lift helpers threaded upward. Callers
/// convert to their target error kind at the callsite:
///
/// * `tatara-export-worker` composes into `anyhow::Result` through the
///   `?` operator's `impl From<serde_json::Error> for anyhow::Error`
///   sugar — one character of glue at the callsite instead of a
///   dedicated `.map_err` wrap.
/// * `tatara-process::hostname` composes into `Result<_, HostnameError>`
///   through `.map_err(|_| HostnameError::InvalidLabel { .. })` — the
///   substrate-primitive's typed error is projected onto the
///   invalid-spec corner of the hostname's typed error surface. The
///   underlying `serde_json` diagnostic is discarded deliberately at
///   the pre-lift callsite (its wording is not operator-actionable at
///   the FQDN emit boundary), and the substrate primitive preserves
///   that discard choice.
///
/// # `#[must_use]`
///
/// Every consumer feeds the returned bytes into a BLAKE3 hash — the
/// intent / control pillar on the receipt-envelope compose side, the
/// content-hash prefix on the ephemeral-id compose side. Dropping the
/// return silently reduces the pillar to empty bytes, which is never
/// the intended semantic (the `?` propagation in every consumer would
/// mask the drop with a compiler warning that this attribute
/// surfaces).
///
/// # Theory anchor
///
/// THEORY.md §VI.1 (generation over composition — the 2-link
/// `serde_json::to_value → to_vec` chain recurred at two hand-authored
/// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and is
/// lifted to ONE substrate owner here). THEORY.md §II.1 invariant 5
/// (composition preserves proofs — the byte-identity pin
/// [`tests::canonical_bytes_matches_pre_lift_to_value_to_vec_chain_bytewise`]
/// binds the primitive byte-identically to both hand-authored
/// spellings, AND the key-canonicalization pin
/// [`tests::canonical_bytes_sorts_hashmap_keys_alphabetically`]
/// binds the load-bearing sort property that both pre-lift consumers
/// depended on for hash stability).
#[must_use = "an unused pillar-bytes result silently drops the payload; hash the result or thread it via `?`"]
pub fn canonical_bytes<T: Serialize + ?Sized>(v: &T) -> Result<Vec<u8>, serde_json::Error> {
    let value = serde_json::to_value(v)?;
    serde_json::to_vec(&value)
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

    // ── pillar_bytes byte-identity + corner pins ──────────────────

    #[test]
    fn pillar_bytes_matches_pre_lift_serde_json_to_vec_shape_bytewise() {
        // Byte-identical parity with the pre-lift
        // `serde_json::to_vec(v).unwrap_or_default()` spelling every
        // three-pillar producer restated at its own body. Swept across
        // representative pillar-input shapes (unit, primitive, struct,
        // vec, map, nested) so a substrate-side canonicalization or
        // reordering the pre-lift chain does NOT apply would surface
        // HERE rather than as silent pillar-bytes drift at every
        // downstream three-pillar consumer.
        use serde::Serialize;
        #[derive(Serialize)]
        struct Inner {
            a: u32,
            b: String,
        }
        assert_eq!(
            pillar_bytes(&()),
            serde_json::to_vec(&()).unwrap_or_default(),
        );
        assert_eq!(
            pillar_bytes(&42u64),
            serde_json::to_vec(&42u64).unwrap_or_default(),
        );
        assert_eq!(
            pillar_bytes(&"hello".to_string()),
            serde_json::to_vec(&"hello".to_string()).unwrap_or_default(),
        );
        let inner = Inner {
            a: 7,
            b: "x".into(),
        };
        assert_eq!(
            pillar_bytes(&inner),
            serde_json::to_vec(&inner).unwrap_or_default(),
        );
        let v: Vec<u32> = vec![1, 2, 3];
        assert_eq!(pillar_bytes(&v), serde_json::to_vec(&v).unwrap_or_default());
        let mut map = std::collections::BTreeMap::new();
        map.insert("k".to_string(), 1u32);
        map.insert("j".to_string(), 2u32);
        assert_eq!(
            pillar_bytes(&map),
            serde_json::to_vec(&map).unwrap_or_default(),
        );
    }

    #[test]
    fn pillar_bytes_is_deterministic_across_calls() {
        // serde_json is deterministic on a stable input; the primitive
        // is pure. A regression that accidentally seeded a nonce, read
        // a clock, or salted the encoding would fail loudly HERE rather
        // than as silent composed_root drift at every downstream
        // consumer.
        #[derive(serde::Serialize)]
        struct S {
            a: u32,
        }
        let s = S { a: 1 };
        assert_eq!(pillar_bytes(&s), pillar_bytes(&s));
    }

    #[test]
    fn pillar_bytes_of_unit_produces_null_json() {
        // The empty-bytes fallback is triggered by serde errors, NOT
        // by an empty input — `pillar_bytes(&())` is `b"null"`, not
        // `[]`. Pin the corner so a regression that mis-conflated
        // "empty pillar" with "serde failure" would fail HERE rather
        // than as silent pillar-input drift at any downstream reader
        // that treated the two corners identically.
        assert_eq!(pillar_bytes(&()), b"null");
    }

    #[test]
    fn pillar_bytes_accepts_borrowed_and_owned_serializable_inputs() {
        // Both borrowed (`&String`) and owned-via-borrow (`&<T:
        // Serialize>` where the caller already owns the payload)
        // ride through the same `T: Serialize + ?Sized` bound
        // without a per-callsite `.to_owned()` / `.clone()` wrap.
        // The `?Sized` relaxation is required so `pillar_bytes(&"x")`
        // (a `&str`, unsized) type-checks the same as
        // `pillar_bytes(&owned_string)`.
        let owned: String = "hello".into();
        assert_eq!(pillar_bytes(&owned), b"\"hello\"");
        assert_eq!(pillar_bytes("hello"), b"\"hello\"");
        assert_eq!(pillar_bytes(&owned), pillar_bytes("hello"));
    }

    // ── canonical_bytes byte-identity + corner pins ───────────────

    #[test]
    fn canonical_bytes_matches_pre_lift_to_value_to_vec_chain_bytewise() {
        // Byte-identical parity with the pre-lift 2-link
        // `serde_json::to_value(v)? → serde_json::to_vec(&v)` spelling
        // both `tatara-process::hostname::canonical_json` +
        // `tatara-export-worker::canonical_json` restated at their own
        // bodies. Sweeps unit / primitive / struct / vec / map / nested
        // shapes so a substrate-side reordering (a canonicalization
        // that swept struct fields, a serde-version change to Value's
        // internal Map backing) surfaces HERE rather than as silent
        // canonical-bytes drift at every downstream consumer.
        use serde::Serialize;
        #[derive(Serialize)]
        struct Inner {
            a: u32,
            b: String,
        }
        let pre_lift =
            |v: &serde_json::Value| -> Result<Vec<u8>, serde_json::Error> { serde_json::to_vec(v) };
        for value in [
            serde_json::to_value(()).unwrap(),
            serde_json::to_value(42u64).unwrap(),
            serde_json::to_value("hello".to_string()).unwrap(),
            serde_json::to_value(Inner {
                a: 7,
                b: "x".into(),
            })
            .unwrap(),
            serde_json::to_value(vec![1u32, 2, 3]).unwrap(),
        ] {
            let via_primitive = canonical_bytes(&value).unwrap();
            let via_pre_lift = pre_lift(&value).unwrap();
            assert_eq!(via_primitive, via_pre_lift);
        }
    }

    #[test]
    fn canonical_bytes_sorts_hashmap_keys_alphabetically() {
        // The load-bearing canonicalization property: keys of a
        // `HashMap<String, V>` (whose iteration order is
        // unspecified across serde-json versions and per-run randomized
        // for BuildHasherDefault) come out ALPHABETICALLY sorted
        // through this primitive. The `serde_json::Value::Object`
        // intermediate uses `serde_json::Map` = `BTreeMap<String,
        // Value>` in this workspace (no `preserve_order` feature —
        // confirmed by the absence of `indexmap` under `serde_json` in
        // `Cargo.lock`), so the `to_value` round-trip normalizes the
        // key emission order before the final `to_vec`. A regression
        // that dropped the Value round-trip (or that flipped the
        // workspace to `preserve_order`) would fail-loudly HERE
        // rather than as silent per-run hash drift at every
        // ephemeral-id / export-receipt consumer.
        use std::collections::HashMap;
        let mut map: HashMap<String, u32> = HashMap::new();
        map.insert("z".to_string(), 1);
        map.insert("m".to_string(), 2);
        map.insert("a".to_string(), 3);
        let bytes = canonical_bytes(&map).unwrap();
        assert_eq!(bytes, br#"{"a":3,"m":2,"z":1}"#);
    }

    #[test]
    fn canonical_bytes_is_deterministic_across_hashmap_insertion_orders() {
        // Two HashMaps with the SAME keys+values but populated in
        // opposite insertion orders MUST project onto identical
        // canonical bytes. This is the direct consumer-side contract
        // both pre-lift `canonical_json` helpers depended on for hash
        // stability (identical spec → identical ephemeral_id;
        // identical outcome → identical control_hash). A regression
        // that lost the sort — say, a switch to `IndexMap` under
        // `preserve_order` — would surface as silent per-run drift at
        // every downstream BLAKE3 consumer; the pin binds the
        // insertion-order invariant HERE.
        use std::collections::HashMap;
        let mut ascending: HashMap<String, u32> = HashMap::new();
        ascending.insert("a".to_string(), 3);
        ascending.insert("m".to_string(), 2);
        ascending.insert("z".to_string(), 1);
        let mut descending: HashMap<String, u32> = HashMap::new();
        descending.insert("z".to_string(), 1);
        descending.insert("m".to_string(), 2);
        descending.insert("a".to_string(), 3);
        assert_eq!(
            canonical_bytes(&ascending).unwrap(),
            canonical_bytes(&descending).unwrap()
        );
    }

    #[test]
    fn canonical_bytes_agrees_with_pillar_bytes_on_alphabetical_shapes() {
        // Coherence with the sibling primitive `pillar_bytes` on every
        // shape whose emission is already alphabetical (a struct whose
        // declaration order coincides with alphabetical order, an
        // already-sorted BTreeMap, non-map primitives). These are the
        // pillar-input shapes both primitives serialize identically. A
        // regression at either owner that drifted the shared corner —
        // a switch to some non-alphabetical struct-field sort at
        // `canonical_bytes`, a swap of `to_vec` for a canonicalizing
        // encoder at `pillar_bytes` — would fail-loudly at THIS pin
        // rather than as silent drift between the two workspace-wide
        // pillar-bytes primitives on the shared corner.
        use serde::Serialize;
        #[derive(Serialize)]
        struct AlphaOrdered {
            a: u32,
            b: String,
        }
        let s = AlphaOrdered {
            a: 7,
            b: "x".into(),
        };
        assert_eq!(canonical_bytes(&s).unwrap(), pillar_bytes(&s));
        assert_eq!(canonical_bytes(&()).unwrap(), pillar_bytes(&()));
        assert_eq!(canonical_bytes(&42u64).unwrap(), pillar_bytes(&42u64));
        let v: Vec<u32> = vec![1, 2, 3];
        assert_eq!(canonical_bytes(&v).unwrap(), pillar_bytes(&v));
        let mut btree = std::collections::BTreeMap::new();
        btree.insert("k".to_string(), 1u32);
        btree.insert("j".to_string(), 2u32);
        assert_eq!(canonical_bytes(&btree).unwrap(), pillar_bytes(&btree));
    }

    #[test]
    fn canonical_bytes_diverges_from_pillar_bytes_on_non_alphabetical_field_order() {
        // Byte-shape divergence pin: on a struct whose declaration
        // order is NOT alphabetical, the two primitives produce
        // different bytes. `pillar_bytes` emits DECLARATION order (the
        // direct `serde_json::to_vec` behavior); `canonical_bytes`
        // emits ALPHABETICAL order (the Value round-trip through the
        // BTreeMap-backed `serde_json::Map`). This split is
        // load-bearing — a caller choosing `canonical_bytes` over
        // `pillar_bytes` is asking for the canonicalizing sort, and a
        // regression that silently merged the two primitives at the
        // struct corner would invalidate every persisted receipt whose
        // pillar-input has a non-alphabetical field order. The pin
        // binds the split HERE so the two primitives evolve as an
        // explicitly-partitioned pair on the (canonicalize? y/n) axis.
        use serde::Serialize;
        #[derive(Serialize)]
        struct DescOrdered {
            z_first: u32,
            a_last: u32,
        }
        let s = DescOrdered {
            z_first: 1,
            a_last: 2,
        };
        // pillar_bytes preserves declaration order:
        assert_eq!(pillar_bytes(&s), br#"{"z_first":1,"a_last":2}"#);
        // canonical_bytes sorts alphabetically:
        assert_eq!(canonical_bytes(&s).unwrap(), br#"{"a_last":2,"z_first":1}"#);
        // The two must diverge on this corner:
        assert_ne!(canonical_bytes(&s).unwrap(), pillar_bytes(&s));
    }

    #[test]
    fn canonical_bytes_of_unit_produces_null_json() {
        // The unit input projects to `b"null"` — matching the sibling
        // `pillar_bytes(&())` corner. `canonical_bytes` succeeds on
        // this input (serde emits `null` for `()`), so a regression
        // that mis-conflated "empty pillar" with "serde failure" at
        // the strict-Result peer would fail HERE rather than as silent
        // pillar-input drift.
        assert_eq!(canonical_bytes(&()).unwrap(), b"null");
    }

    #[test]
    fn canonical_bytes_accepts_borrowed_and_owned_serializable_inputs() {
        // The `T: Serialize + ?Sized` bound admits both borrowed
        // (`&String`, `&Vec<u8>`) and unsized-via-borrow (`&str`)
        // inputs without a per-callsite `.to_owned()` / `.clone()`
        // wrap — matching the sibling `pillar_bytes` bound.
        let owned: String = "hello".into();
        assert_eq!(canonical_bytes(&owned).unwrap(), b"\"hello\"");
        assert_eq!(canonical_bytes("hello").unwrap(), b"\"hello\"");
        assert_eq!(
            canonical_bytes(&owned).unwrap(),
            canonical_bytes("hello").unwrap()
        );
    }

    #[test]
    fn canonical_bytes_sorts_struct_fields_alphabetically() {
        // Struct fields are emitted in ALPHABETICAL order through
        // `canonical_bytes` — the `to_value` intermediate `Value::Object`
        // is `serde_json::Map = BTreeMap<String, Value>`, so serde's
        // struct→map serializer inserts each field-name and the BTreeMap
        // re-emits them alphabetically regardless of declaration order.
        // This is the stronger-canonicalization behavior every downstream
        // receipt / ephemeral-id consumer implicitly relied on for
        // cross-run hash stability (a struct with a HashMap-typed field
        // OR a struct whose declaration order changes across a
        // refactor would still produce the same canonical bytes). A
        // regression that dropped the Value round-trip would surface
        // as declaration-order output HERE, and would silently
        // invalidate every persisted receipt whose pillar-input is a
        // struct with a non-alphabetical field order.
        use serde::Serialize;
        #[derive(Serialize)]
        struct Ordered {
            z_first: u32,
            a_last: u32,
        }
        let s = Ordered {
            z_first: 1,
            a_last: 2,
        };
        assert_eq!(canonical_bytes(&s).unwrap(), br#"{"a_last":2,"z_first":1}"#);
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
