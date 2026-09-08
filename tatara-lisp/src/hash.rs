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
use std::fmt::Display;

/// The canonical `"blake3:"` scheme prefix every three-pillar-style
/// wire-form BLAKE3 hash string in the workspace opens with — the
/// workspace-wide ONE substrate owner of the scheme literal on the
/// READ (predicate) AND WRITE (compose) axes.
///
/// ## Why the substrate lives here
///
/// Pre-lift the SAME `"blake3:"` bare literal was hand-authored across
/// two WRITE-side call sites in `tatara-ui` (paired with a `ShortHash:
/// Display` receiver at `Renderer::artifact` + `Renderer::summary`,
/// each composing `format!("blake3:{<hash>}")` past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold) AND at the negative-form
/// READ pin `!h.starts_with("blake3:")` inside
/// [`tests::hex_blake3_of_json_is_64_lowercase_hex_chars`]. An earlier
/// round routed both WRITE sites through `tatara-ui::hash::
/// blake3_scheme_display` — a peer owner opened LOCAL to `tatara-ui`.
/// That opener resolved the two `tatara-ui` write sites but left the
/// scheme literal owned at the CLI-UX-crate layer; every
/// `tatara-lisp`-depending crate downstream of the identity-projection
/// axis (`tatara-nix::store::StoreHash` producers, `tatara-terreiro`
/// snapshot-identity emitters, any future `#[derive(TataraDomain)]`
/// consumer that wraps a bare BLAKE3 hex into the canonical
/// three-pillar wire form) that reached for the same scheme-prefixed
/// wrap would either re-author `format!("blake3:{X}", …)` OR take a
/// new dep on `tatara-ui`. Neither is right: the wrap step is a
/// property of the hash-identity substrate, not of the CLI-UX crate.
///
/// This module resolves the split by placing the substrate at the ONE
/// crate every current + future consumer of the identity-projection
/// axis already depends on — [`crate::hash::hex_blake3_of_json`] (the
/// bare-hex sibling) lives here for the exact same reason, and the
/// two now live at ONE canonical owner on the (bare, scheme-prefixed)
/// axis. Post-lift `tatara-ui::hash::BLAKE3_SCHEME_PREFIX` +
/// `tatara-ui::hash::blake3_scheme_display` become thin `pub use`
/// re-export delegates keeping in-crate spellings (`crate::hash::…`)
/// stable while the canonical owner sits alongside the bare-hex
/// sibling.
///
/// ## Sibling owner in `tatara-engine`
///
/// [`tatara-engine::domain::attestation::pillar_hash`] owns the
/// COMPUTE-and-WRAP axis (`&[u8] → "blake3:{hex}"`) — the primitive
/// that composes the scheme prefix WITH a freshly-computed
/// `blake3::Hash`. That owner cannot fold into this one because
/// `tatara-engine` does not depend on `tatara-lisp` (an intentional
/// dep-graph choice: the engine crate carries the seven-driver
/// executor + Raft/gossip planes, and the Lisp reader has no place
/// in that graph). This owner partitions the same scheme-prefix
/// surface at the (compute-and-wrap, wrap-only) axis, matching the
/// existing partition between `tatara-process::hash::hex_blake3` (the
/// flat one-shot BLAKE3-hex on a byte buffer, sibling to
/// [`hex_blake3_of_json`] on the value-input axis) and the
/// three-pillar `pillar_hash` composer.
///
/// Theory anchor: THEORY.md §II.1 invariant 5 (composition preserves
/// proofs — the wire-form scheme literal at ONE substrate owner
/// means every WRITE-side wrap and every READ-side predicate agree
/// bytewise by construction). THEORY.md §V.3 (three-pillar
/// attestation — the canonical pillar-hash shape is `"blake3:{hex}"`;
/// this constant pins the scheme prefix half of that shape on the
/// wrap-only axis).
pub const BLAKE3_SCHEME_PREFIX: &str = "blake3:";

/// The [`BLAKE3_SCHEME_PREFIX`]-prefixed wire form of an already-
/// computed BLAKE3 hex handle — the workspace-wide ONE substrate
/// owner of the `format!("blake3:{<hex>}")` one-line wrap chain every
/// WRITE-side consumer that needs the canonical scheme-prefixed wire
/// form (without re-computing the underlying digest) restated by hand
/// pre-lift.
///
/// ## Pre-lift consumers
///
/// The SAME chain was hand-authored at TWO workspace-visible WRITE
/// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, each
/// composing a `ShortHash`'s dim-styled render slot:
///
/// * `tatara_ui::render::Renderer::artifact` — the per-artifact line's
///   `◇ blake3:<hash>` slot, paired with the dim-styled artifact name +
///   state chunk. The `ShortHash: Display` receiver stamped a 7-char
///   BLAKE3 prefix onto the line, wrapped verbatim in the scheme
///   prefix via `format!("blake3:{hash}")`.
/// * `tatara_ui::render::Renderer::summary` — the summary banner's
///   content-root slot, paired with the totals line. The same
///   `ShortHash: Display` receiver stamped the root-hash prefix onto
///   the banner, wrapped verbatim in the scheme prefix via
///   `format!("blake3:{root_hash}")`.
///
/// Both sites walked the SAME one-link chain — take a `Display`-
/// projectable BLAKE3 hex handle and prepend the substrate's canonical
/// [`BLAKE3_SCHEME_PREFIX`] — differing only in the receiver's slot
/// name (`hash` vs `root_hash`, both `&ShortHash`). Post-lift each
/// callsite reads `blake3_scheme_display(<receiver>)` through
/// `tatara-ui`'s `pub use` re-export delegate; the wrap step lives at
/// ONE canonical owner here.
///
/// ## `H: Display`
///
/// The receiver bound is `H: Display` so the primitive accepts BOTH
/// the pre-lift `ShortHash` receiver (which implements `Display`
/// writing its own 7-char BLAKE3 prefix) AND future consumers that
/// produce a raw `String` / `&str` / `blake3::Hash` hex handle. The
/// `format!("{BLAKE3_SCHEME_PREFIX}{hex}")` composition is polymorphic
/// on the `Display` axis so every current caller compiles verbatim
/// through the substrate primitive.
///
/// ## `#[must_use]`
///
/// Every consumer either stamps the returned scheme-prefixed string
/// into a `Renderer::text(...)` slot (both current callers) or feeds
/// it directly onto a wire — dropping the return means the wrap was
/// performed for no observable reason.
///
/// ## Byte-shape parity
///
/// Byte-shape parity with the pre-lift hand-authored one-line chain
/// is pinned at
/// [`tests::blake3_scheme_display_matches_pre_lift_format_scheme_chain_bytewise`]
/// so a regression that reshaped the scheme literal (a `b3:`
/// shorthand, an uppercase `BLAKE3:` variant), swapped the format
/// positional (`format!("{hex}{}", "blake3:")` — hex before scheme),
/// or added a separator (`"blake3: "` with a stray space) surfaces
/// HERE rather than as silent operator-facing drift at every WRITE
/// consumer.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// one-line `format!("blake3:{<hex>}")` chain recurred at TWO
/// hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// trigger, and is lifted to ONE substrate owner here). THEORY.md
/// §V.3 (three-pillar attestation — the canonical pillar-hash shape
/// is `"blake3:{hex}"`; this primitive owns the wrap-only half of
/// that shape at the workspace root, sibling to the compute-and-wrap
/// owner `pillar_hash` in `tatara-engine`).
#[must_use]
pub fn blake3_scheme_display<H: Display>(hex: H) -> String {
    format!("{BLAKE3_SCHEME_PREFIX}{hex}")
}

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
    use super::{blake3_scheme_display, hex_blake3_of_json, BLAKE3_SCHEME_PREFIX};
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

    // ── blake3_scheme_display substrate pins ─────────────────────────
    //
    // Each pin below binds [`blake3_scheme_display`] +
    // [`BLAKE3_SCHEME_PREFIX`] at fail-before-pass-after granularity
    // so a regression that reshaped the scheme literal (a `b3:`
    // shorthand, an uppercase `BLAKE3:` variant), swapped the format
    // positional (hex before scheme), stray-spaced the separator
    // (`"blake3: "`), or dropped the wrap entirely surfaces HERE
    // rather than as silent operator-facing drift at every WRITE-side
    // render/emit consumer downstream of the identity-projection axis.

    /// Byte-identical parity with the pre-lift hand-authored
    /// `format!("blake3:{<hex>}")` chain both WRITE-side consumers in
    /// `tatara-ui` walked (`Renderer::artifact` +
    /// `Renderer::summary`). Sweeps representative `H: Display`
    /// receivers so the polymorphism composes byte-identically to
    /// the pre-lift monomorphic shape across every corner: bare
    /// `&str`, owned `String`, and `blake3::Hash: Display` (the
    /// tatara-engine three-pillar corner that would consume this
    /// primitive if the dep-graph allowed it — the test proves the
    /// shape polymorphism is honest across the (fixed, computed)
    /// axis).
    #[test]
    fn blake3_scheme_display_matches_pre_lift_format_scheme_chain_bytewise() {
        // Bare &str receiver — a consumer that has a raw hex handle
        // without any newtype wrap.
        let raw = "abcd1234";
        assert_eq!(
            blake3_scheme_display(raw),
            format!("blake3:{raw}"),
            "blake3_scheme_display drifted from pre-lift `format!(\"blake3:{{}}\", _)` on &str",
        );

        // Owned String receiver — the same shape as &str but by-value
        // through the `Display` bound.
        let owned: String = "deadbeef".into();
        assert_eq!(
            blake3_scheme_display(&owned),
            format!("blake3:{owned}"),
            "blake3_scheme_display drifted from pre-lift chain on &String",
        );

        // 64-char lowercase-hex receiver — matching the shape of a
        // `hex_blake3_of_json(v)` output the WRITE-side wrap primitive
        // composes on top of.
        let long = "0".repeat(64);
        assert_eq!(
            blake3_scheme_display(&long),
            format!("blake3:{long}"),
            "blake3_scheme_display drifted from pre-lift chain on 64-char hex",
        );
    }

    /// Wire-format pin: every output MUST begin with the canonical
    /// [`BLAKE3_SCHEME_PREFIX`] byte sequence. A regression that
    /// dropped the prefix (writing bare hex) or drifted the spelling
    /// (`b3:`, `BLAKE3:`, `blake3=`) would silently break downstream
    /// consumers whose regex or prefix-strip step expects the exact
    /// `"blake3:"` scheme.
    #[test]
    fn blake3_scheme_display_output_carries_blake3_scheme_prefix() {
        let out = blake3_scheme_display("cxx3i50");
        assert!(
            out.starts_with(BLAKE3_SCHEME_PREFIX),
            "blake3_scheme_display output must carry the `{BLAKE3_SCHEME_PREFIX}` scheme prefix, got {out:?}",
        );
        // Empty-hex corner — the wrap is unconditional, so an empty
        // receiver still produces the bare scheme prefix. Pin the
        // corner so a future refactor that added a `hex.is_empty()`
        // short-circuit has to explicitly move this pin.
        assert_eq!(blake3_scheme_display(""), BLAKE3_SCHEME_PREFIX);
    }

    /// Determinism pin: `blake3_scheme_display` is a pure `format!`
    /// composition, so two calls with the same receiver produce
    /// byte-identical output. A regression that mixed nondeterminism
    /// in (a wall-clock read, a random seed, a per-fleet suffix)
    /// would surface HERE rather than as flaky render output.
    #[test]
    fn blake3_scheme_display_is_deterministic_for_identical_receiver() {
        let receiver = "deadbeefcafe";
        let a = blake3_scheme_display(receiver);
        let b = blake3_scheme_display(receiver);
        assert_eq!(a, b);
    }

    /// Length pin: the wrap prepends `BLAKE3_SCHEME_PREFIX.len()`
    /// bytes onto its receiver's `Display` output. Pin the wire-shape
    /// budget so a downstream fixed-width alignment assumption
    /// surfaces here rather than as visual drift at the render slot.
    #[test]
    fn blake3_scheme_display_output_length_is_prefix_plus_receiver_length() {
        for receiver in ["", "x", "cxx3i50", &"0".repeat(64)] {
            let out = blake3_scheme_display(receiver);
            assert_eq!(
                out.len(),
                BLAKE3_SCHEME_PREFIX.len() + receiver.len(),
                "wrap output length must be prefix+receiver for {receiver:?}",
            );
            assert!(out.starts_with(BLAKE3_SCHEME_PREFIX));
        }
    }

    /// Cross-consumer coherence: the WRITE-side wrap primitive AND
    /// the READ-side scheme-absence assertion (used at
    /// [`hex_blake3_of_json_is_64_lowercase_hex_chars`]) share the
    /// SAME canonical scheme constant. A regression that changed the
    /// constant's VALUE (`"blake3:"` → `"b3:"`) would land the WRITE
    /// output on the new prefix AND the READ predicate would accept
    /// the new prefix as its scheme-absent floor, keeping both sides
    /// in lockstep at ONE substrate owner. Pins the invariant the
    /// two-way partition between [`hex_blake3_of_json`] (bare hex,
    /// no prefix) and [`blake3_scheme_display`] (prefix+hex) rides.
    #[test]
    fn blake3_scheme_display_wrap_and_read_side_pin_share_scheme_constant() {
        let out = blake3_scheme_display("someHex");
        assert!(
            out.starts_with(BLAKE3_SCHEME_PREFIX),
            "WRITE-side wrap primitive must carry the substrate's canonical scheme prefix"
        );
        // The bare-hex projection at `hex_blake3_of_json` produces
        // 64-lowercase-hex-only, and the WRITE wrap primitive adds
        // the scheme prefix on top. Both sides route through
        // `BLAKE3_SCHEME_PREFIX`.
        #[derive(Serialize)]
        struct F {
            k: u32,
        }
        let bare = hex_blake3_of_json(&F { k: 0 });
        assert!(
            !bare.starts_with(BLAKE3_SCHEME_PREFIX),
            "bare hex projection must NOT carry the scheme prefix (that's the wrap primitive's role)",
        );
        // The wrap primitive composed on top of the bare projection
        // produces the scheme-prefixed wire form byte-identically.
        let wrapped = blake3_scheme_display(&bare);
        assert_eq!(wrapped, format!("{BLAKE3_SCHEME_PREFIX}{bare}"));
    }

    /// Constant-value pin: the canonical scheme prefix IS the
    /// seven-byte ASCII literal `"blake3:"`. Pins the shape of the
    /// constant so a regression that widened it (a length tag, a
    /// version discriminator suffix) surfaces HERE and every
    /// downstream READ / WRITE consumer sees the change through the
    /// ONE substrate owner.
    #[test]
    fn blake3_scheme_prefix_is_the_canonical_seven_byte_ascii_literal() {
        assert_eq!(BLAKE3_SCHEME_PREFIX, "blake3:");
        assert_eq!(BLAKE3_SCHEME_PREFIX.len(), 7);
        assert!(BLAKE3_SCHEME_PREFIX.is_ascii());
    }
}
