//! Substrate primitive over the `serde_json::to_vec(v)
//! .unwrap_or_default()` + `hex::encode(blake3::hash(&bytes).as_bytes())`
//! two-line chain every `T: Serialize` → BLAKE3-hex identity slot in
//! this crate restated by hand pre-lift.
//!
//! Pre-lift the SAME two-line chain was hand-authored at TWO workspace-
//! visible sites in `tatara-ui` past the ★★ PRIME-DIRECTIVE ≥ 2
//! duplication threshold — each projecting a `T: Serialize` value onto
//! its 64-lowercase-hex BLAKE3 identity string:
//!
//! * [`crate::theme::ThemeSpec::id`] — the content-addressable
//!   [`crate::theme::ThemeId`] a `(deftheme …)` spec derives. Feeds
//!   the `ThemeRegistry::resolve` cache key + the identity token
//!   `tatara replay <theme-id>` receives.
//! * [`crate::event::EventStream::run_hash`] — the run-identity
//!   [`String`] a `Vec<UiEvent>` produces. Feeds the `tatara replay
//!   <hash>` reproduce-past-run path pinned in the crate's own
//!   `stream_run_hash_is_deterministic` test.
//!
//! Both sites walked the SAME two-link chain — serialize `self` via
//! `serde_json::to_vec` with `.unwrap_or_default()` for the residual-
//! error corner, then hand the byte slice to
//! `hex::encode(blake3::hash(&bytes).as_bytes())` — differing only in
//! how the returned 64-char `String` is subsequently wrapped
//! ([`crate::theme::ThemeId`]-newtype at the theme site, bare `String`
//! at the event-stream site). Post-lift each callsite reads
//! `hex_blake3_of_json(self)` and the two-link chain lives at ONE
//! substrate owner.
//!
//! Sibling in shape to `tatara_process::hash::hex_blake3` (the flat
//! `hex::encode(blake3::hash(bytes).as_bytes())` half — the byte-input
//! projector without the `serde_json::to_vec` prefix) and to
//! `tatara_process::three_pillar::pillar_bytes` (the `serde_json::to_vec
//! (v).unwrap_or_default()` half — the value-to-bytes projector without
//! the hex-BLAKE3 tail). Those two live in `tatara-process` because
//! that crate owns the three-pillar attestation surface; this in-crate
//! composer partitions the same shape by SITE (a UI-run identity, not
//! a K8s process attestation pillar) so `tatara-ui` — a leaf CLI-UX
//! crate that does not otherwise reach for `tatara-process`'s K8s
//! surface — closes the composition here rather than dragging in the
//! kube-rs + k8s-openapi transitive dep graph for a hash function.
//!
//! ### `T: Serialize + ?Sized`
//!
//! The `?Sized` relaxation matches the sibling
//! `tatara_process::three_pillar::pillar_bytes` bound so the primitive
//! accepts BOTH owned receivers (`&ThemeSpec`, `&EventStream`) AND
//! unsized borrows (`&str` — a future consumer that reaches for the
//! BLAKE3-hex of a string literal without an owned re-materialization).
//! Every consumer receives its handle through a `&` reference, so the
//! bound is not load-bearing at the two current callsites — but
//! removing it would silently reject the `&str` corner at the type
//! level.
//!
//! ### `#[must_use]`
//!
//! Every consumer either stores the returned hex into an identity
//! newtype ([`crate::theme::ThemeId`]) or feeds it directly to a
//! `tatara replay <hash>` command line. Dropping the return means the
//! hash was computed for no observable reason; the attribute surfaces
//! that as a warning at every consumer site.
//!
//! Theory anchor: THEORY.md §V.3 (three-pillar attestation — the
//! canonical `serde_json → BLAKE3 → hex` value-identity projection is
//! defined at the substrate; this composer routes `tatara-ui`'s two
//! run/theme-identity slots onto the SAME shape). THEORY.md §VI.1
//! (generation over composition — the two-line chain recurred at TWO
//! hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
//! trigger, and is lifted to ONE substrate owner here).

use serde::Serialize;

/// The lowercase-64-hex BLAKE3 digest of `v`'s canonical JSON
/// serialization — the ONE substrate owner of the two-line
/// `serde_json::to_vec(v).unwrap_or_default()` +
/// `hex::encode(blake3::hash(&bytes).as_bytes())` chain every
/// `T: Serialize` → identity-string projector in this crate walks.
///
/// # Invariants
///
/// - **Length:** the returned string is always exactly 64 chars
///   (BLAKE3's 32-byte digest encoded as lowercase hex).
/// - **Charset:** every char is one of `[0-9a-f]` (lowercase).
/// - **Determinism:** byte-identical output across runs for the same
///   input value's canonical JSON — pinned at
///   [`tests::hex_blake3_of_json_is_deterministic_for_identical_input`].
/// - **Residual arm:** a value whose `serde::Serialize` impl returns
///   an error (unreachable for every current consumer whose derived
///   `Serialize` is infallible) hashes the empty byte slice — matching
///   the pre-lift `.unwrap_or_default()` corner byte-for-byte.
///
/// Byte-shape parity with the pre-lift hand-authored two-line chain is
/// pinned at
/// [`tests::hex_blake3_of_json_matches_pre_lift_hand_authored_chain_bytewise`],
/// so a regression that reshaped the internal composition (a switch to
/// `blake3::hash(x).to_hex().to_string()`, a swap of `hex::encode`'s
/// `.as_bytes()` slice for a different byte-projection) still passes
/// only because it observably produces the same bytes; the pin fixes
/// the OBSERVABLE contract.
#[must_use]
pub fn hex_blake3_of_json<T: Serialize + ?Sized>(v: &T) -> String {
    let bytes = serde_json::to_vec(v).unwrap_or_default();
    hex::encode(blake3::hash(&bytes).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::hex_blake3_of_json;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Fixture {
        name: String,
        n: u32,
    }

    /// Fail-before-pass-after granularity: the two-line hash-composition
    /// chain that pre-lift lived at [`crate::theme::ThemeSpec::id`] +
    /// [`crate::event::EventStream::run_hash`] MUST match the primitive
    /// [`hex_blake3_of_json`] byte-for-byte for every observable
    /// receiver. A regression that reshaped the internal composition
    /// (a switch to `blake3::hash(x).to_hex().to_string()`, a hex-crate
    /// swap, a dropped `.unwrap_or_default()` residual-arm) still passes
    /// only because it observably produces the same bytes; the pin
    /// fixes the OBSERVABLE contract.
    #[test]
    fn hex_blake3_of_json_matches_pre_lift_hand_authored_chain_bytewise() {
        for spec in [
            Fixture {
                name: "nord-arctic".into(),
                n: 0,
            },
            Fixture {
                name: "solarized".into(),
                n: 42,
            },
            Fixture {
                name: String::new(),
                n: u32::MAX,
            },
        ] {
            // Byte-identical to the pre-lift hand-authored chain both
            // callsites walked.
            let pre_lift = {
                let bytes = serde_json::to_vec(&spec).unwrap_or_default();
                hex::encode(blake3::hash(&bytes).as_bytes())
            };
            assert_eq!(
                hex_blake3_of_json(&spec),
                pre_lift,
                "hex_blake3_of_json drift from pre-lift chain for {:?}",
                spec.name,
            );
        }
    }

    /// Same input value → same hash. Both pre-lift sites depended on
    /// this determinism corner (`ThemeSpec::id` for a cache key, the
    /// `stream_run_hash_is_deterministic` test on the sibling site).
    #[test]
    fn hex_blake3_of_json_is_deterministic_for_identical_input() {
        let spec = Fixture {
            name: "deterministic".into(),
            n: 7,
        };
        let a = hex_blake3_of_json(&spec);
        let b = hex_blake3_of_json(&spec);
        assert_eq!(a, b);
    }

    /// Distinct inputs → distinct hashes. Pins the "no accidental
    /// collision at the hex layer" invariant a bare `.unwrap_or_default()`
    /// residual arm never triggers on infallible-Serialize receivers.
    #[test]
    fn hex_blake3_of_json_distinct_inputs_hash_distinct() {
        let a = hex_blake3_of_json(&Fixture {
            name: "a".into(),
            n: 1,
        });
        let b = hex_blake3_of_json(&Fixture {
            name: "b".into(),
            n: 1,
        });
        assert_ne!(a, b);
    }

    /// Output shape: 64 lowercase-hex chars for every observable input,
    /// no leading `blake3:` prefix (that scheme prefix lives on the
    /// three-pillar attestation composer in `tatara-engine`, not on the
    /// bare hex projector every consumer here reaches for). Pins the
    /// invariant the pre-lift `hex::encode` step produced.
    #[test]
    fn hex_blake3_of_json_is_64_lowercase_hex_chars() {
        for spec in [
            Fixture {
                name: "shape".into(),
                n: 1,
            },
            Fixture {
                name: String::new(),
                n: 0,
            },
        ] {
            let h = hex_blake3_of_json(&spec);
            assert_eq!(
                h.len(),
                64,
                "hash for {:?} was {} chars",
                spec.name,
                h.len()
            );
            assert!(
                h.chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "hash for {:?} had non-lowercase-hex chars: {h:?}",
                spec.name,
            );
            assert!(
                !h.starts_with("blake3:"),
                "hash for {:?} must not carry the three-pillar `blake3:` scheme prefix",
                spec.name,
            );
        }
    }

    /// `?Sized` reach: a `&str` receiver composes without an owned
    /// re-materialization. Pins the `Serialize + ?Sized` bound choice
    /// — a regression that tightened the bound to `Serialize + Sized`
    /// would silently reject this call at the type level.
    #[test]
    fn hex_blake3_of_json_accepts_unsized_str_receiver() {
        let owned = String::from("hello");
        assert_eq!(
            hex_blake3_of_json::<str>(&owned),
            hex_blake3_of_json(&owned)
        );
        assert_eq!(
            hex_blake3_of_json::<str>("hello"),
            hex_blake3_of_json(&owned)
        );
    }
}
