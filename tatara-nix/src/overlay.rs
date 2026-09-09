//! `Overlay` — nixpkgs's `self: super: …` pattern, typed.
//!
//! An overlay extends or replaces definitions in a package set. In Nix, it's
//! an anonymous function; here it's a named typed record of changes. Composing
//! overlays is the lattice join at the package-set level.

use serde::{Deserialize, Serialize};
use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

use crate::derivation::Derivation;

/// What an overlay targets — the scope of its mutations.
///
/// Substrate primitive over the (kind → canonical `&'static str` label)
/// axis of the overlay-target closed set — the ONE substrate owner of
/// the three-variant PascalCase alphabet (`"PackageSet"` /
/// `"PerSystem"` / `"Module"`) every writer (Lisp `(defoverlay :target
/// <kind>)` authoring, serde `PascalCase` wire-form) and reader
/// ([`crate::overlay_compose::compose`]'s [`ComposeError::TargetMismatch`]
/// operator diagnostic, any future overlay-target dashboard / LSP
/// completion / iac-forge canonical-form renderer) walks on opposite
/// sides of the same wire.
///
/// Pre-lift the variant listing was declaration-order-only: every
/// consumer that wanted to enumerate the closed set (a diagnostic that
/// says "known targets: PackageSet, PerSystem, Module", a coherence
/// check that iterates the alphabet, a `filter_map` over an authored
/// tag stream that resolves each candidate to its typed variant) had
/// to hand-author the three-literal array + the per-variant
/// canonical-string projection at its own callsite past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold — silently coupled by
/// exact-case ASCII agreement to serde's PascalCase output. Post-lift
/// the sweep + the projection + the parse-decode + the operator-facing
/// diagnostic prose all thread through ONE
/// [`tatara_lisp::ClosedSet`] surface + ONE
/// [`std::str::FromStr`] delegation + ONE
/// [`std::fmt::Display`] projection, and adding a fourth variant
/// (`ClusterSet`, `PerArch`, a hypothetical `AppOverlay` for an
/// aplicacao chart overlay slot) lands as ONE entry in [`Self::ALL`] +
/// ONE arm on [`Self::as_str`] — the forced-arity `[Self; N]` array
/// literal makes the compiler check the two surfaces agree at
/// declaration time, and every downstream consumer inherits the new
/// variant mechanically through the trait.
///
/// Sibling closed-set primitives across the tatara typescape:
/// [`tatara_process::phase::ProcessPhase`] (Pending / Forking /
/// Execing / Running / Attested / Reconverging / Exiting / Zombie /
/// Reaped / Failed on the K8s wire-form phase axis),
/// [`tatara_process::boundary::ConditionKind`] (8 variants of the
/// boundary-condition closed set), [`tatara_process::intent::IntentKind`]
/// (6 variants of the process-intent closed set),
/// [`tatara_process::lifetime::LifetimeKind`] (Permanent / Ephemeral
/// on the lifetime-kind axis), [`tatara_process::signal::ProcessSignal`]
/// (7 variants of the process-signal closed set),
/// [`tatara_process::k8s_condition::K8sConditionStatus`] (True / False /
/// Unknown on the K8s ConditionStatus axis) — all follow the SAME
/// four-piece shape (`ALL` const + inherent projection method + `Unknown`
/// carrier + [`std::str::FromStr`] delegation).
///
/// Theory anchor: THEORY.md §III (typescape — the overlay-target
/// closed set is a first-class Rust enum with a compile-time-enforced
/// arity, not a stringly-typed serde tag). THEORY.md §V.1 (knowable
/// platform — the closed set exposes a typed vocabulary every
/// consumer routes through, so a rename or a variant addition
/// propagates mechanically). THEORY.md §VI.1 (generation over
/// composition — the four-piece shape is auto-derived through
/// [`tatara_lisp::DeriveClosedSet`] rather than hand-rolled per
/// implementor).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Default,
    tatara_lisp::DeriveClosedSet,
)]
#[closed_set(via = "as_str", display, generate_unknown)]
pub enum OverlayTarget {
    /// Top-level packages (`pkgs.foo`).
    #[default]
    PackageSet,
    /// Per-system subset (`pkgs.aarch64-linux.foo`).
    PerSystem,
    /// A specific module's options.
    Module,
}

impl OverlayTarget {
    /// The closed set of overlay-target kinds — single source of truth
    /// that drives the [`Self::as_str`] projection, the auto-derived
    /// [`tatara_lisp::ClosedSet::parse_label`] sweep, and the
    /// [`std::fmt::Display`] projection. Adding a fourth variant lands
    /// at ONE `ALL` entry + ONE `as_str` arm — the forced-arity
    /// `[Self; 3]` array literal makes the compiler verify the two
    /// surfaces agree at declaration time.
    ///
    /// Sibling `ALL` sweeps across the tatara typescape:
    /// [`tatara_process::phase::ProcessPhase::ALL`],
    /// [`tatara_process::boundary::ConditionKind::ALL`],
    /// [`tatara_process::intent::IntentKind::ALL`],
    /// [`tatara_process::signal::ProcessSignal::ALL`],
    /// [`tatara_process::k8s_condition::K8sConditionStatus::ALL`].
    pub const ALL: [Self; 3] = [Self::PackageSet, Self::PerSystem, Self::Module];

    /// Canonical PascalCase wire-form projection — matches the serde
    /// unit-variant output verbatim (`PackageSet` → `"PackageSet"`,
    /// `PerSystem` → `"PerSystem"`, `Module` → `"Module"`) so a
    /// consumer that reads either projection sees byte-identical
    /// output. Load-bearing on: (1) the auto-derived
    /// [`tatara_lisp::ClosedSet::parse_label`] sweep the substrate
    /// routes every canonical-label decode through, (2) the
    /// derive-emitted [`std::fmt::Display`] projection every
    /// operator-facing diagnostic composes through, and (3) the
    /// serde-parity anchor a future JSON-shape parity pin binds on.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PackageSet => "PackageSet",
            Self::PerSystem => "PerSystem",
            Self::Module => "Module",
        }
    }
}

// `impl fmt::Display for OverlayTarget` + `impl FromStr for
// OverlayTarget` + `impl tatara_lisp::ClosedSet for OverlayTarget` +
// `pub struct UnknownOverlayTarget(pub String)` are generated by
// `#[derive(tatara_lisp::DeriveClosedSet)]` + `#[closed_set(via =
// "as_str", display, generate_unknown)]` on the enum declaration
// above. The auto-derived label `"overlay target"` (spaced-lowercase
// projection of the PascalCase enum name) threads into the carrier's
// `#[error("unknown overlay target: {0}")]` annotation.

/// An overlay — a named bundle of additions + replacements.
///
/// ```lisp
/// (defoverlay add-gnu-patches
///   :target PackageSet
///   :adds   ((:name "hello-enhanced" :version "2.12.1-plus-patches"))
///   :replaces (("hello" (:name "hello" :version "2.12.1-ga-patched"))))
/// ```
#[derive(DeriveTataraDomain, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defoverlay")]
pub struct Overlay {
    pub name: String,
    #[serde(default)]
    pub target: OverlayTarget,
    /// Brand-new packages introduced by this overlay.
    #[serde(default)]
    pub adds: Vec<Derivation>,
    /// Replacements: each entry is `(upstream-name, new-derivation)`.
    #[serde(default)]
    pub replaces: Vec<Replacement>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Replace an existing package's derivation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Replacement {
    pub upstream_name: String,
    pub with: Derivation,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use tatara_lisp::{domain::TataraDomain, read, ClosedSet};

    #[test]
    fn minimal_overlay_compiles() {
        let forms = read(
            r#"(defoverlay
                  :name "patched"
                  :target PackageSet
                  :description "carries a local patch")"#,
        )
        .unwrap();
        let o = Overlay::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(o.name, "patched");
        assert_eq!(o.target, OverlayTarget::PackageSet);
        assert!(o.adds.is_empty());
    }

    // ── OverlayTarget closed-set algebra pins ────────────────────────
    //
    // Fail-before-pass-after granularity: the `ALL` const, the
    // `as_str` projection, the `ClosedSet` trait impl, the derived
    // `FromStr` delegation, the derived `Display` projection, and the
    // auto-generated `UnknownOverlayTarget` carrier did NOT exist
    // before this commit — every test below fails to compile pre-lift.
    // Post-lift they collectively pin the four-piece closed-set-enum
    // shape at ONE substrate owner across the (variant enumeration,
    // canonical projection, parse-decode, Display, unknown carrier)
    // surfaces so a regression at any face surfaces here rather than
    // as silent operator-facing drift at every consumer that iterates
    // the closed set.

    /// Structural well-formedness of [`OverlayTarget`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins every structural invariant of the
    /// closed-set idiom (non-empty `ALL`, round-trip through `label`
    /// ↔ `parse_label`, pairwise-distinct labels, reserved-probe
    /// rejection, `find_by_label` / `contains_label` / `sorted_labels`
    /// / `labels_joined` composition consistency, `SET_LABEL` threading
    /// into the carrier's Display shape) at ONE call site. Replaces
    /// hand-rolled `all_is_unique_and_complete` +
    /// `roundtrip_via_as_str` + empty-rejection sweeps — the substrate
    /// primitive checks every default composition the trait exposes
    /// through ONE `assert_closed_set_well_formed::<T>()` sweep.
    #[test]
    fn overlay_target_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<OverlayTarget>();
    }

    /// CANONICAL-KEY CONTRACT: [`OverlayTarget::as_str`] matches
    /// serde's PascalCase unit-variant output verbatim for every
    /// variant. A future variant rename (or an `as_str` arm typo)
    /// silently desynchronizes the Lisp `(defoverlay :target <kind>)`
    /// authoring surface from the JSON wire-form the reader path
    /// deserializes — this pin surfaces the drift at the substrate
    /// primitive rather than as silent operator-facing wire-shape
    /// skew across every persisted overlay.
    #[test]
    fn as_str_matches_serde_unit_variant_shape_bytewise() {
        for v in OverlayTarget::ALL {
            let via_serde = serde_json::to_string(&v).unwrap();
            let via_as_str = format!("\"{}\"", v.as_str());
            assert_eq!(
                via_serde, via_as_str,
                "as_str drift from serde output on {v:?}",
            );
        }
    }

    /// [`OverlayTarget::ALL`] sweeps every variant in declaration
    /// order — pinned three ways to close the (arity, uniqueness,
    /// cover) matrix at fail-before-pass-after granularity:
    ///
    /// 1. **Arity** — the array's length equals the closed set's
    ///    cardinality (3). The `[Self; 3]` type-level arity already
    ///    forces this at the substrate; this runtime witness catches
    ///    a regression that widened the type to `&[Self]` or a
    ///    `Vec<Self>` builder.
    /// 2. **No duplicates** — the collected set has cardinality equal
    ///    to the sweep's length. A copy-paste (`[PackageSet,
    ///    PackageSet, Module]`) surfaces at the cardinality check.
    /// 3. **Declaration-order** — the sweep matches the enum's
    ///    declaration order, so a future consumer that binds to the
    ///    sweep ordering (a config-decoder default, a dashboard
    ///    layout, an LSP completion priority) sees a stable ordering
    ///    across every workspace consumer.
    #[test]
    fn all_covers_the_overlay_target_closed_set_exhaustively() {
        assert_eq!(
            OverlayTarget::ALL.len(),
            3,
            "ALL must enumerate every variant of the overlay-target closed set — \
             a regression that added a variant at `as_str` but forgot to extend \
             `ALL` surfaces here",
        );
        let seen: std::collections::HashSet<OverlayTarget> =
            OverlayTarget::ALL.iter().copied().collect();
        assert_eq!(
            seen.len(),
            OverlayTarget::ALL.len(),
            "ALL must not stamp any variant twice — a copy-paste at the sweep surfaces here",
        );
        assert_eq!(
            OverlayTarget::ALL,
            [
                OverlayTarget::PackageSet,
                OverlayTarget::PerSystem,
                OverlayTarget::Module,
            ],
            "ALL must preserve declaration order — every downstream consumer \
             that binds to the ordering inherits it through this sweep",
        );
    }

    /// [`OverlayTarget::as_str`] emits byte-identical PascalCase
    /// literals to the pre-lift hand-authored enum-variant names. A
    /// regression that lower-cased one variant or drifted the ASCII
    /// wire-form at ONE arm surfaces HERE, not as silent parse-decode
    /// failure at the [`tatara_lisp::ClosedSet::parse_label`] sweep
    /// or JSON round-trip.
    #[test]
    fn as_str_matches_pre_lift_pascal_case_literals_bytewise() {
        assert_eq!(OverlayTarget::PackageSet.as_str(), "PackageSet");
        assert_eq!(OverlayTarget::PerSystem.as_str(), "PerSystem");
        assert_eq!(OverlayTarget::Module.as_str(), "Module");
    }

    /// The derive-emitted [`std::fmt::Display`] projection composes
    /// through [`OverlayTarget::as_str`] byte-for-byte — the two
    /// projections are interchangeable at every consumer. Pins the
    /// invariant that a caller who reaches for the stdlib Display
    /// conversion path (via `.to_string()`, `format!("{v}")`, a
    /// `write!` macro) gets the same wire-form bytes as a direct
    /// `.as_str()` call.
    #[test]
    fn display_composes_through_as_str_bytewise() {
        for v in OverlayTarget::ALL {
            assert_eq!(v.to_string(), v.as_str());
            assert_eq!(format!("{v}"), v.as_str());
        }
    }

    /// Round-trip through the derive-emitted [`std::str::FromStr`]
    /// delegation yields the SAME variant every time. Pins the
    /// invariant that writing `v.as_str()` then parsing the output
    /// recovers `v` for every variant — the parse-decode arm the
    /// [`tatara_lisp::ClosedSet::parse_label`] sweep routes through
    /// stays symmetric with the [`OverlayTarget::as_str`] projection.
    #[test]
    fn from_str_round_trip_holds_for_every_variant() {
        for v in OverlayTarget::ALL {
            let parsed = OverlayTarget::from_str(v.as_str()).unwrap();
            assert_eq!(parsed, v, "round-trip drift on {v:?}");
        }
    }

    /// The derive-emitted [`std::str::FromStr`] rejects strings
    /// outside the closed-set alphabet — case-drift, whitespace-
    /// wrapped variants, empty string, unrelated literals. The
    /// carrier echoes the offending input verbatim so an operator
    /// grepping the diagnostic sees the SAME payload the parser
    /// rejected. Pins the closed-set nature: `from_str` is a total
    /// function over the wire-form alphabet, not a permissive parser
    /// that accepts synonyms.
    #[test]
    fn from_str_rejects_case_drift_and_out_of_vocabulary_inputs() {
        for bad in [
            "",
            "packageset",
            "PACKAGESET",
            "Package_Set",
            " PackageSet",
            "PackageSet ",
            "packages-set",
            "AttrSet",
            "System",
            "module",
        ] {
            let err = OverlayTarget::from_str(bad).unwrap_err();
            assert_eq!(
                err.0, bad,
                "UnknownOverlayTarget must echo input verbatim, got {err:?}",
            );
        }
    }

    /// The auto-generated `UnknownOverlayTarget` carrier's
    /// `thiserror::Error` derive renders the substrate-wide
    /// `"unknown <spaced-lowercase enum name>: <input>"` shape —
    /// `PascalCase` → `"overlay target"` via the derive's
    /// `pascal_to_spaced_lowercase` projection. A regression at the
    /// derive (a case-drifted spacing pass, a hand-rolled carrier
    /// that dropped the substrate-wide `"unknown"` prefix) surfaces
    /// HERE rather than as silent operator-facing diagnostic-shape
    /// skew across every downstream consumer.
    #[test]
    fn unknown_carrier_renders_substrate_wide_diagnostic_shape() {
        let err = OverlayTarget::from_str("bogus").unwrap_err();
        assert_eq!(err.to_string(), "unknown overlay target: bogus");
    }

    /// [`OverlayTarget::SET_LABEL`] on the derive-emitted
    /// [`tatara_lisp::ClosedSet`] impl exposes the spaced-lowercase
    /// noun phrase the substrate-wide `"unknown {SET_LABEL}:
    /// {input}"` diagnostic threads into. Pinning the projection at
    /// the trait means a future generic consumer (a metrics tagger,
    /// a workspace-wide typed-completion surface) reads the SAME
    /// label the carrier's `#[error(...)]` annotation carries — the
    /// two surfaces flow from ONE generative origin at the derive.
    #[test]
    fn set_label_matches_pascal_case_spacing_projection() {
        assert_eq!(<OverlayTarget as ClosedSet>::SET_LABEL, "overlay target",);
    }

    /// The derive-emitted [`tatara_lisp::ClosedSet::find_by_label`]
    /// zero-allocation typed decode routes every canonical label to
    /// its typed variant WITHOUT materializing a throwaway
    /// [`UnknownOverlayTarget`] carrier on the reject path. Pins
    /// symmetry with [`OverlayTarget::from_str`] (which pays the
    /// carrier allocation) so a future consumer that needs the
    /// variant WITHOUT the diagnostic (a `filter_map` over an
    /// authored tag stream, an LSP hover pass) binds to the trait
    /// method rather than paying an unused `String` allocation.
    #[test]
    fn find_by_label_zero_alloc_decode_matches_from_str_on_canonical_input() {
        for v in OverlayTarget::ALL {
            assert_eq!(
                <OverlayTarget as ClosedSet>::find_by_label(v.as_str()),
                Some(v),
            );
        }
        assert_eq!(<OverlayTarget as ClosedSet>::find_by_label("bogus"), None,);
        // Zero-allocation peer never touches the carrier constructor
        // on reject — the return type is `Option`, not `Result`.
    }

    /// [`tatara_lisp::ClosedSet::CARDINALITY`] on the derive-emitted
    /// trait impl equals [`OverlayTarget::ALL`]'s length. Pins the
    /// (variant listing, variant count) axis: a future consumer that
    /// needs the cardinality at a compile-time-const site (a
    /// per-variant lookup table `[Payload; T::CARDINALITY]`, a
    /// bitset dimension) binds to ONE trait const rather than
    /// re-deriving `T::ALL.len()` inline at every callsite.
    #[test]
    fn cardinality_matches_all_length_on_the_trait() {
        assert_eq!(
            <OverlayTarget as ClosedSet>::CARDINALITY,
            OverlayTarget::ALL.len(),
        );
        assert_eq!(<OverlayTarget as ClosedSet>::CARDINALITY, 3);
    }
}
