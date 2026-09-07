//! Thin delegate onto [`tatara_lisp::hash::hex_blake3_of_json`] — the
//! workspace-wide ONE substrate owner of the `serde_json::to_vec(v)
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
//! ### Post-lift redirect (workspace-wide unification)
//!
//! Since a follow-up round routed `tatara-terreiro::compute_id` and
//! `tatara-nix::store::StoreHash::of` — the third and fourth
//! consumers of the same identity-projection shape — the substrate is
//! now owned at `tatara_lisp::hash::hex_blake3_of_json`, upstream of
//! every current consumer (`tatara-lisp` is depended on by every
//! `#[derive(TataraDomain)]` crate). This function stays here as a
//! thin re-export delegate so in-crate callsites keep their local
//! `crate::hash::hex_blake3_of_json` spelling, but the composition is
//! authored at the workspace root of the identity-projection axis.
//! Production code in this crate no longer reaches for `blake3` or
//! `hex` directly (the substrate does), so both demote to
//! `[dev-dependencies]` in the manifest — a future re-introduction of
//! the two-line chain here would ALSO need to re-promote both deps,
//! surfacing the drift at review time.
//!
//! Sibling in shape to `tatara_process::hash::hex_blake3` (the flat
//! `hex::encode(blake3::hash(bytes).as_bytes())` half — the byte-input
//! projector without the `serde_json::to_vec` prefix) and to
//! `tatara_process::three_pillar::pillar_bytes` (the `serde_json::to_vec
//! (v).unwrap_or_default()` half — the value-to-bytes projector without
//! the hex-BLAKE3 tail). Those two live in `tatara-process` because
//! that crate owns the three-pillar attestation surface; the workspace-
//! wide value-to-hex-BLAKE3 identity projector lives at
//! `tatara_lisp::hash` because every downstream consumer already
//! depends on `tatara-lisp` for its `#[derive(TataraDomain)]`.
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
use std::fmt::Display;

/// The canonical `"blake3:"` scheme prefix every three-pillar-style
/// wire-form BLAKE3 hash string in the workspace opens with — a typed
/// substrate owner of the scheme literal on the READ (predicate) AND
/// WRITE (compose) axes.
///
/// Pre-lift the SAME `"blake3:"` bare literal was hand-authored across
/// two axes of consumer sites in this crate:
///
/// * WRITE — [`crate::render::Renderer::artifact`] +
///   [`crate::render::Renderer::summary`]: each composed the same
///   `format!("blake3:{<hash>}")` wire-form wrap on top of a
///   `ShortHash: Display` receiver to paint the `◇ blake3:<hash>` slot
///   next to every artifact + summary line, past the ★★
///   PRIME-DIRECTIVE ≥ 2 duplication threshold.
/// * READ — [`tests::hex_blake3_of_json_is_64_lowercase_hex_chars`]'s
///   `!h.starts_with("blake3:")` negative-form pin (the scheme-prefix-
///   absence invariant every consumer of the bare
///   [`hex_blake3_of_json`] projector inherits) already reads the same
///   bare literal on the predicate side.
///
/// Post-lift the two WRITE sites route through [`blake3_scheme_display`]
/// and the READ pin references this constant, so a future scheme change
/// (a length tag, a version discriminator, a per-fleet suffix, a
/// `b3:` shorthand) lands at ONE substrate owner rather than at the
/// two consumers-that-remembered-to-spell-it-right.
///
/// Sibling owner to the `"blake3:"` scheme literal
/// [`tatara-engine::domain::attestation::pillar_hash`] carries on the
/// COMPUTE-and-WRAP axis (`&[u8] → "blake3:{hex}"`): that primitive
/// composes the scheme prefix with a freshly-computed BLAKE3 digest;
/// [`blake3_scheme_display`] composes the scheme prefix with an
/// ALREADY-computed hex handle. The two owners partition the
/// scheme-prefix surface at the (compute-and-wrap, wrap-only) axis.
///
/// Theory anchor: THEORY.md §II.1 invariant 5 (composition preserves
/// proofs — the wire-form scheme literal at ONE substrate owner means
/// the render path and its assertion pins agree bytewise by
/// construction). THEORY.md §V.3 (three-pillar attestation — the
/// canonical pillar-hash shape is `"blake3:{hex}"`; this constant pins
/// the scheme prefix half of that shape at the UI-side wire).
pub const BLAKE3_SCHEME_PREFIX: &str = "blake3:";

/// The [`BLAKE3_SCHEME_PREFIX`]-prefixed wire form of an already-
/// computed BLAKE3 hex handle — the ONE substrate owner of the
/// `format!("blake3:{<hex>}")` one-line wrap chain the render surface
/// restated at TWO WRITE sites pre-lift.
///
/// Pre-lift the SAME chain was hand-authored at TWO workspace-visible
/// WRITE sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger,
/// each composing a `ShortHash`'s dim-styled render slot:
///
/// * [`crate::render::Renderer::artifact`] — the per-artifact line's
///   `◇ blake3:<hash>` slot, paired with the dim-styled artifact name +
///   state chunk. The `ShortHash: Display` receiver stamped a 7-char
///   BLAKE3 prefix onto the line, wrapped verbatim in the scheme
///   prefix via `format!("blake3:{hash}")`.
/// * [`crate::render::Renderer::summary`] — the summary banner's
///   content-root slot, paired with the totals line. The same
///   `ShortHash: Display` receiver stamped the root-hash prefix onto
///   the banner, wrapped verbatim in the scheme prefix via
///   `format!("blake3:{root_hash}")`.
///
/// Both sites walked the SAME one-link chain — take a `Display`-
/// projectable BLAKE3 hex handle and prepend the substrate's canonical
/// [`BLAKE3_SCHEME_PREFIX`] — differing only in the receiver's slot
/// name (`hash` vs `root_hash`, both `&ShortHash`). Post-lift each
/// callsite reads `blake3_scheme_display(<receiver>)` and the wrap
/// step lives at ONE substrate owner sharing the scheme literal with
/// its READ-side sibling pin.
///
/// # `H: Display`
///
/// The receiver bound is `H: Display` so the primitive accepts BOTH
/// the pre-lift receivers ([`crate::event::ShortHash`], which
/// implements `Display` writing its own 7-char BLAKE3 prefix) AND
/// future consumers that produce a raw `String` / `&str` / `blake3::
/// Hash` hex handle. The `format!("{}{hex}", BLAKE3_SCHEME_PREFIX)`
/// composition is polymorphic on the `Display` axis so every current
/// caller compiles verbatim through the substrate primitive.
///
/// # `#[must_use]`
///
/// Every consumer either stamps the returned scheme-prefixed string
/// into a `Renderer::text(...)` slot (both current callers) or feeds
/// it directly onto a wire — dropping the return means the wrap was
/// performed for no observable reason.
///
/// Byte-shape parity with the pre-lift hand-authored one-line chain is
/// pinned at
/// [`tests::blake3_scheme_display_matches_pre_lift_format_scheme_chain_bytewise`]
/// so a regression that reshaped the scheme literal (a `b3:` shorthand,
/// an uppercase `BLAKE3:` variant), swapped the format positional
/// (`format!("{hex}{}", "blake3:")` — hex before scheme), or added a
/// separator (`"blake3: "` with a stray space) surfaces HERE rather
/// than as silent operator-facing drift at the render slot.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// one-line `format!("blake3:{<hex>}")` chain recurred at TWO
/// hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// trigger, and is lifted to ONE substrate owner here). THEORY.md §V.3
/// (three-pillar attestation — the canonical pillar-hash shape is
/// `"blake3:{hex}"`; this primitive owns the wrap-only half of that
/// shape at the UI-side wire, sibling to the compute-and-wrap owner
/// `pillar_hash` in tatara-engine).
#[must_use]
pub fn blake3_scheme_display<H: Display>(hex: H) -> String {
    format!("{BLAKE3_SCHEME_PREFIX}{hex}")
}

/// The lowercase-64-hex BLAKE3 digest of `v`'s canonical JSON
/// serialization — a thin delegate onto
/// [`tatara_lisp::hash::hex_blake3_of_json`], the workspace-wide ONE
/// substrate owner of the two-line
/// `serde_json::to_vec(v).unwrap_or_default()` +
/// `hex::encode(blake3::hash(&bytes).as_bytes())` chain every
/// `T: Serialize` → identity-string projector walks. This re-export
/// keeps in-crate callsites spelled as
/// `crate::hash::hex_blake3_of_json` for locality while the byte-
/// projection is authored at the workspace root.
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
    tatara_lisp::hash::hex_blake3_of_json(v)
}

#[cfg(test)]
mod tests {
    use super::{blake3_scheme_display, hex_blake3_of_json, BLAKE3_SCHEME_PREFIX};
    use crate::event::ShortHash;
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
                // Route the scheme-absence READ pin through the same
                // substrate constant [`BLAKE3_SCHEME_PREFIX`] the
                // WRITE-side primitive [`blake3_scheme_display`]
                // composes onto. A future scheme rename lands at ONE
                // owner and both sides move in lockstep.
                !h.starts_with(BLAKE3_SCHEME_PREFIX),
                "hash for {:?} must not carry the three-pillar `{BLAKE3_SCHEME_PREFIX}` scheme prefix",
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
    // Bind [`blake3_scheme_display`] at fail-before-pass-after
    // granularity so a regression that swapped the scheme literal
    // (a `b3:` shorthand, an uppercase `BLAKE3:` variant), reversed the
    // format positional (hex before scheme), stray-spaced the
    // separator (`"blake3: "`), or dropped the wrap entirely surfaces
    // HERE rather than as silent operator-facing drift at the render
    // slot every artifact + summary line paints.
    //
    // Each pin is fail-before-pass-after: the primitive did not exist
    // pre-lift, so any test that invokes it fails to compile pre-lift
    // and passes post-lift; the byte-identity pins below then bind the
    // specific shape choice.

    /// Byte-identical parity with the pre-lift hand-authored
    /// `format!("blake3:{<hex>}")` chain both WRITE-side callers walked.
    /// A regression that reshaped the scheme literal, swapped the
    /// format positional, or added a stray separator surfaces HERE
    /// rather than as a silent operator-facing drift at the render
    /// slot. Sweeps representative receivers — a `ShortHash` (matching
    /// the two pre-lift callsite shapes), a bare `&str`, a `String` —
    /// so the `H: Display` polymorphism composes byte-identically to
    /// the pre-lift monomorphic shape across every corner.
    #[test]
    fn blake3_scheme_display_matches_pre_lift_format_scheme_chain_bytewise() {
        // ShortHash receiver — mirrors the two pre-lift callsite
        // shapes at `Renderer::artifact` + `Renderer::summary`.
        let short = ShortHash::from_blake3_hex("cxx3i50lvlprhlqclm1m");
        let via_primitive = blake3_scheme_display(&short);
        let via_pre_lift = format!("blake3:{short}");
        assert_eq!(
            via_primitive, via_pre_lift,
            "blake3_scheme_display drifted from pre-lift `format!(\"blake3:{{}}\", _)` on ShortHash",
        );

        // Bare &str receiver — a future consumer that has a raw
        // hex handle without a ShortHash newtype wrap.
        let raw = "abcd1234";
        assert_eq!(blake3_scheme_display(raw), format!("blake3:{raw}"));

        // Owned String receiver — the same shape as &str but by-value
        // through the `Display` bound.
        let owned: String = "deadbeef".into();
        assert_eq!(blake3_scheme_display(&owned), format!("blake3:{owned}"));
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
        let short = ShortHash::from_blake3_hex("deadbeefcafe");
        let a = blake3_scheme_display(&short);
        let b = blake3_scheme_display(&short);
        assert_eq!(a, b);
    }

    /// Length pin: for a `ShortHash` receiver (7-char BLAKE3 prefix
    /// via `ShortHash::from_blake3_hex`'s `.chars().take(7).collect()`
    /// step), the wrapped output is exactly
    /// `BLAKE3_SCHEME_PREFIX.len() + 7` chars. Pin the wire-shape
    /// budget so a `Renderer::text(...)` slot's fixed-width alignment
    /// assumption surfaces here rather than as a visual drift.
    #[test]
    fn blake3_scheme_display_short_hash_output_is_scheme_plus_seven_char_prefix() {
        let short = ShortHash::from_blake3_hex("cxx3i50lvlprhlqc");
        let out = blake3_scheme_display(&short);
        assert_eq!(
            out.len(),
            BLAKE3_SCHEME_PREFIX.len() + 7,
            "blake3_scheme_display on ShortHash must produce scheme+7-char-prefix wire form, got {out:?}",
        );
        assert!(out.starts_with(BLAKE3_SCHEME_PREFIX));
    }

    /// Cross-consumer coherence: the WRITE-side wrap primitive AND the
    /// READ-side scheme-absence assertion (used by
    /// [`hex_blake3_of_json_is_64_lowercase_hex_chars`]) share the SAME
    /// canonical scheme prefix. A regression that drifted ONE side
    /// (e.g. renamed the constant from under the WRITE primitive
    /// without updating the READ pin) would silently pass through this
    /// pin — but a regression that changed the constant's VALUE
    /// (`"blake3:"` → `"b3:"`) would land the WRITE output on the
    /// new prefix AND the READ predicate would accept the new prefix
    /// as its scheme-absent floor, keeping both sides in lockstep at
    /// ONE substrate owner.
    #[test]
    fn blake3_scheme_display_wrap_and_read_side_pin_share_scheme_constant() {
        let out = blake3_scheme_display("someHex");
        assert!(
            out.starts_with(BLAKE3_SCHEME_PREFIX),
            "WRITE-side wrap primitive must carry the substrate's canonical scheme prefix"
        );
        // The READ-side pin at
        // `hex_blake3_of_json_is_64_lowercase_hex_chars` asserts a
        // bare-hex output does NOT start with the SAME constant. Pin
        // the coherence: the bare-hex projection at
        // `hex_blake3_of_json` produces 64-lowercase-hex-only, and the
        // WRITE wrap primitive adds the scheme prefix on top. Both
        // sides route through `BLAKE3_SCHEME_PREFIX`.
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
}
