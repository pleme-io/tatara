//! Substrate primitive over the two-line `serde_json::to_vec(v)
//! .unwrap_or_default()` + `hex::encode(blake3::hash(&bytes).as_bytes())`
//! chain every `T: Serialize` → 64-lowercase-hex BLAKE3 identity slot
//! restated by hand pre-lift across the workspace.
//!
//! ## Why the substrate lives here
//!
//! Pre-lift the SAME chain was hand-authored at THREE workspace-visible
//! sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold — each
//! projecting a `T: Serialize` value onto its 64-lowercase-hex BLAKE3
//! identity string:
//!
//! * `tatara_ui::theme::ThemeSpec::id` — the content-addressable
//!   [`ThemeId`](../../tatara_ui/theme/struct.ThemeId.html) a
//!   `(deftheme …)` spec derives.
//! * `tatara_ui::event::EventStream::run_hash` — the run-identity
//!   `String` a `Vec<UiEvent>` produces.
//! * `tatara_nix::store::StoreHash::of` — the content-addressable
//!   `StoreHash` a `serde::Serialize` value produces, feeding
//!   [`StorePath::hash`](../../tatara_nix/store/struct.StorePath.html)
//!   and every Nix-derivation identity slot in that crate.
//!
//! In an earlier round the first two collapsed onto
//! `tatara_ui::hash::hex_blake3_of_json` (commit `6f8f9ca`) and
//! `tatara-terreiro`'s sealed-snapshot identity was routed through the
//! same substrate (commit `ed61792`). The third site — `tatara-nix`'s
//! `StoreHash::of` — was deferred that run because `tatara-nix` does
//! not depend on `tatara-ui` (an intentional dep-graph choice: the
//! store crate should not reach for a CLI-UX crate carrying
//! `owo-colors` + `libc` transitively), leaving the workspace with an
//! unresolved third re-authoring.
//!
//! This module resolves that split by placing the substrate at the
//! ONE crate every current consumer already depends on — `tatara-lisp`
//! is the trait+registry+reader root of the typed-Lisp surface, and
//! both `tatara-ui` (for `#[derive(TataraDomain)]`) and `tatara-nix`
//! (for `#[derive(TataraDomain)]` on `Derivation`, `Module`, etc.)
//! already reach for it. Promoting the substrate here inverts the
//! dep-graph question: no consumer takes on a new dep, and the
//! canonical owner sits at the same layer as the [`crate::domain`] +
//! [`crate::reader`] surface those consumers already compose over.
//!
//! ## Sibling shapes
//!
//! Sibling in shape to `tatara_process::hash::hex_blake3` (the flat
//! `hex::encode(blake3::hash(bytes).as_bytes())` half — the byte-input
//! projector without the `serde_json::to_vec` prefix) and to
//! `tatara_process::three_pillar::pillar_bytes` (the `serde_json::to_vec
//! (v).unwrap_or_default()` half — the value-to-bytes projector without
//! the hex-BLAKE3 tail). Those two live in `tatara-process` because
//! that crate owns the three-pillar K8s attestation surface; this
//! composer partitions the same shape by SITE — the workspace-wide
//! value-to-hex-BLAKE3 identity projector consumed by anything that
//! isn't a K8s three-pillar pillar.
//!
//! ## Byte-shape parity
//!
//! Internally uses [`blake3::Hash::to_hex`] rather than the
//! `hex::encode(hash.as_bytes())` chain the pre-lift sites walked.
//! Both spellings produce a 32-byte digest encoded as exactly 64
//! lowercase ASCII hex chars — byte-identical output. The parity is
//! pinned at
//! [`tests::hex_blake3_of_json_matches_pre_lift_hex_encode_chain_bytewise`]
//! using `hex` as a dev-dep, so a regression in either spelling (a
//! blake3 major-version bump that reshapes `Hash::to_hex`, a hex-crate
//! swap of `encode`'s case) surfaces at this pin rather than as silent
//! drift downstream.
//!
//! ## `T: Serialize + ?Sized`
//!
//! The `?Sized` relaxation matches the sibling
//! `tatara_process::three_pillar::pillar_bytes` bound so the primitive
//! accepts BOTH owned receivers (`&ThemeSpec`, `&StoreHash`) AND
//! unsized borrows (`&str` — a future consumer that reaches for the
//! BLAKE3-hex of a string literal without an owned re-materialization).
//! Every current consumer receives its handle through a `&` reference,
//! so the bound is not load-bearing at the three current callsites —
//! but removing it would silently reject the `&str` corner at the
//! type level.
//!
//! ## `#[must_use]`
//!
//! Every consumer either stores the returned hex into an identity
//! newtype (`ThemeId`, `StoreHash`) or feeds it directly to a
//! `tatara replay <hash>` command line. Dropping the return means the
//! hash was computed for no observable reason; the attribute surfaces
//! that as a warning at every consumer site.
//!
//! Theory anchor: THEORY.md §V.3 (three-pillar attestation — the
//! canonical `serde_json → BLAKE3 → hex` value-identity projection is
//! defined at the substrate; this composer routes every workspace
//! run/theme/store identity slot onto the SAME shape). THEORY.md §VI.1
//! (generation over composition — the two-line chain recurred at THREE
//! hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
//! trigger, and is lifted to ONE substrate owner here).

use serde::Serialize;

/// The lowercase-64-hex BLAKE3 digest of `v`'s canonical JSON
/// serialization — the workspace-wide ONE substrate owner of the
/// two-line `serde_json::to_vec(v).unwrap_or_default()` +
/// `hex::encode(blake3::hash(&bytes).as_bytes())` chain every
/// `T: Serialize` → identity-string projector walks.
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
/// Byte-shape parity with the pre-lift hand-authored
/// `hex::encode(blake3::hash(...).as_bytes())` chain is pinned at
/// [`tests::hex_blake3_of_json_matches_pre_lift_hex_encode_chain_bytewise`].
#[must_use]
pub fn hex_blake3_of_json<T: Serialize + ?Sized>(v: &T) -> String {
    let bytes = serde_json::to_vec(v).unwrap_or_default();
    blake3::hash(&bytes).to_hex().to_string()
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

    /// Fail-before-pass-after: the substrate's
    /// [`blake3::Hash::to_hex`]-backed spelling produces byte-identical
    /// output to the pre-lift hand-authored
    /// `hex::encode(blake3::hash(&bytes).as_bytes())` chain every
    /// upstream consumer walked. A regression in either the blake3
    /// crate's `Hash::to_hex` (a major-version bump reshaping the
    /// `ArrayString<64>` output) or the hex crate's `encode` (a case
    /// swap, an alternate encoding scheme) surfaces HERE, not as
    /// silent drift at every downstream identity slot.
    #[test]
    fn hex_blake3_of_json_matches_pre_lift_hex_encode_chain_bytewise() {
        for spec in [
            Fixture {
                name: "arctic".into(),
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
            let pre_lift = {
                let bytes = serde_json::to_vec(&spec).unwrap_or_default();
                hex::encode(blake3::hash(&bytes).as_bytes())
            };
            assert_eq!(
                hex_blake3_of_json(&spec),
                pre_lift,
                "hex_blake3_of_json drifted from pre-lift hex::encode chain for {:?}",
                spec.name,
            );
        }
    }

    /// Same input value → same hash. Every consumer (ThemeSpec::id
    /// cache-key, StoreHash content-addressed store-path) depends on
    /// this determinism corner.
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
    /// collision at the hex layer" invariant a bare
    /// `.unwrap_or_default()` residual arm never triggers on
    /// infallible-Serialize receivers.
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

    /// Output shape: exactly 64 lowercase-hex chars for every
    /// observable input, no leading scheme prefix (that prefix lives
    /// on the three-pillar composer in `tatara-engine`, not on the
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
