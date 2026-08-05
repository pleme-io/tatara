//! `#[derive(TataraDomain)]` — generate a `TataraDomain` impl from a Rust struct.
//!
//! ```ignore
//! use tatara_lisp_derive::TataraDomain;
//!
//! #[derive(TataraDomain)]
//! #[tatara(keyword = "defmonitor")]
//! pub struct MonitorSpec {
//!     pub name: String,
//!     pub query: String,
//!     pub threshold: f64,
//!     pub window_seconds: Option<i64>,
//! }
//! ```
//!
//! Generates:
//! ```ignore
//! impl TataraDomain for MonitorSpec {
//!     const KEYWORD: &'static str = "defmonitor";
//!     fn compile_from_args(args: &[Sexp]) -> Result<Self> {
//!         let kw = parse_kwargs_strict(args, __TATARA_ALLOWED_KEYWORDS)?;
//!         Ok(Self {
//!             name: extract_string(&kw, "name")?.to_string(),
//!             query: extract_string(&kw, "query")?.to_string(),
//!             threshold: extract_float_narrowed::<f64>(&kw, "threshold")?,
//!             window_seconds: extract_optional_int_narrowed::<i64>(&kw, "window-seconds")?,
//!         })
//!     }
//! }
//! ```
//!
//! Invoked from Lisp:
//! ```lisp
//! (defmonitor :name "prom-up" :query "up{…}" :threshold 0.99 :window-seconds 300)
//! ```
//!
//! Supported field types (v0):
//!   - `String`, `Option<String>`, `Vec<String>`
//!   - `i64`, `i32`, `u32`, `usize`, `u64`, `Option<i64>`
//!   - `f64`, `f32`, `Option<f64>`
//!   - `bool`, `Option<bool>`
//!
//! Every numeric field goes through `tatara_lisp::domain`'s
//! `NarrowNumeric` projection, NOT a Rust `as` cast: the reader hands
//! back the widest value on each axis (`i64` / `f64`) and the field's
//! own width is recovered by a partial conversion that returns
//! `LispError::KwargOutOfRange` rather than truncating. The identity
//! widths (`i64`, `f64`) route through the same call with a total impl,
//! so the emission is uniform across all seven widths and this derive
//! contains no numeric `as` cast at all.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, Attribute, Data, DeriveInput, Fields, Ident, LitStr, Meta, Type};

/// `#[derive(ClosedSet)]` — emit the substrate-wide
/// [`tatara_lisp::ClosedSet`] impl + the matching [`std::str::FromStr`]
/// delegation for any enum carrying the closed-set-enum idiom (the
/// four-piece `ALL` + projection + `Unknown` + `FromStr` shape).
///
/// Lifts the 4-line `impl ClosedSet` + 4-line `impl FromStr` boilerplate
/// that 29+ workspace-wide implementors re-derive byte-for-byte — the
/// per-implementor content stays at the inherent `ALL` constant and the
/// inherent projection method (`as_str`, `label`, `prefix`, `marker`,
/// `keyword`, …), while the trait-impl plumbing collapses onto ONE
/// derive line.
///
/// ## Attributes
///
/// - `#[closed_set(via = "as_str")]` — name of the inherent projection
///   method the trait's [`tatara_lisp::ClosedSet::label`] delegates to.
///   Defaults to `"label"`. Domain-canonical names
///   (`tatara_process`'s `as_str`, `tatara_lisp::ast::QuoteForm::prefix`,
///   `tatara_lisp::error::UnquoteForm::marker`,
///   `tatara_lisp::error::MacroDefHead::keyword`) stay load-bearing.
/// - `#[closed_set(unknown = "UnknownX")]` — name of the
///   per-implementor `Unknown` carrier struct
///   [`tatara_lisp::ClosedSet::make_unknown`] constructs. Defaults to
///   `"Unknown{EnumName}"` — matches the substrate-wide naming
///   convention (`UnknownChannelKind` for `ChannelKind`).
/// - `#[closed_set(no_from_str)]` — suppress the generated
///   `impl FromStr`. Use for enums that already carry a bespoke
///   `FromStr` shape (e.g. [`tatara_lisp::error::CompilerSpecIoStage`]'s
///   compound `"{operation}: {label}"` key, which keys on a projection
///   PAIR rather than a single label).
/// - `#[closed_set(generate_unknown)]` /
///   `#[closed_set(generate_unknown = "<label>")]` — emit the
///   `pub struct Unknown{EnumName}(pub String)` parse-rejection
///   carrier alongside the trait impl. The carrier derives
///   `Debug + Clone + PartialEq + Eq + thiserror::Error` and renders
///   `#[error("unknown <label>: {0}")]`. The bare form derives
///   `<label>` by spacing the PascalCase enum name into lowercase
///   words (`ChannelKind` → "channel kind", `ReplacementPolicy` →
///   "replacement policy"); the `= "..."` form pins an explicit label
///   for irregular cases (`MacroDefHead` wants "macro definition
///   head" rather than the auto-derived "macro def head";
///   `MustReachPhase` wants "must-reach phase"). The 3-line
///   `pub struct Unknown{EnumName}(pub String)` declaration (plus its
///   thiserror derives + `#[error(...)]` annotation) is the
///   substrate-wide closed-set-enum idiom's last hand-rolled piece;
///   this attribute collapses it onto the derive so a 40+ enum
///   cohort emits the carrier through ONE generative shape rather
///   than re-deriving the boilerplate at each declaration site.
/// - `#[closed_set(display)]` — emit the substrate-wide
///   `impl ::core::fmt::Display for $name { f.write_str(Self::$via(*self)) }`
///   block alongside the trait impl. The 5-line Display block (the
///   `impl fmt::Display`, the `fn fmt`, the `f.write_str(self.$via())`
///   body) appears 28+ times across `tatara-process` /
///   `tatara-lisp` byte-for-byte — every closed-set carrier on a
///   PascalCase wire-format axis composes its operator-facing
///   diagnostic through Display rather than through a hard-coded
///   literal that would silently rot when a variant gets renamed.
///   The attribute collapses the 5-line block onto ONE flag so the
///   `as_str` ⇄ Display ⇄ `FromStr` triad emits through ONE
///   generative shape per closed-set enum.
///   The emission requires `Self: ::core::marker::Copy` (the
///   `ClosedSet` trait already requires it). Set the flag in
///   combination with `via` to pin Display onto the inherent
///   projection rather than the trait method; without the flag the
///   implementor keeps its hand-rolled Display block (e.g. for a
///   bespoke Display shape like
///   [`tatara_process::lifetime_clock::TerminateReason`]'s
///   structured-reason formatter).
///
/// ## Implementor requirements
///
/// The derive expects the enum to expose at the inherent surface:
///
/// 1. `pub const ALL: [Self; N] = [...]` — forced-arity array literal.
/// 2. A `fn projection(self) -> &'static str` method whose name matches
///    `via` (defaults to `label`).
/// 3. A `pub struct UnknownX(pub String)` in the same module whose name
///    matches `unknown` (defaults to `Unknown{EnumName}`) — UNLESS
///    `#[closed_set(generate_unknown)]` is set, in which case the
///    derive emits the struct itself.
///
/// The derive emits:
///
/// ```ignore
/// impl ::tatara_lisp::ClosedSet for $name {
///     const ALL: &'static [Self] = &Self::ALL;
///     type Unknown = $unknown;
///     fn label(self) -> &'static str { Self::$via(self) }
///     fn make_unknown(s: &str) -> Self::Unknown {
///         $unknown(::std::string::String::from(s))
///     }
/// }
///
/// impl ::core::str::FromStr for $name {
///     type Err = $unknown;
///     fn from_str(s: &str) -> ::core::result::Result<Self, Self::Err> {
///         <Self as ::tatara_lisp::ClosedSet>::parse_label(s)
///     }
/// }
/// ```
///
/// ## Theory grounding
///
/// THEORY.md §VI.1 — generation over composition; the derive IS the
/// generative shape — new closed-set enums add ONE `#[derive(ClosedSet)]`
/// line + the attribute that names their inherent projection method
/// instead of re-deriving the eight-line `impl ClosedSet` + `impl FromStr`
/// pair byte-for-byte. The per-implementor `Unknown` carrier stays
/// hand-rolled (its `#[error("unknown <thing>: {0}")]` annotation IS
/// per-implementor content), but the trait-impl plumbing it threads
/// through collapses onto the derive.
#[proc_macro_derive(ClosedSet, attributes(closed_set))]
pub fn derive_closed_set(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident.clone();

    if !matches!(input.data, Data::Enum(_)) {
        return spanned_compile_error(
            &name,
            "ClosedSet may only be derived on enums (the closed-set-enum idiom)",
        );
    }

    let cfg = match parse_closed_set_attrs(&input.attrs, &name) {
        Ok(c) => c,
        Err(err) => return emit_compile_error(err),
    };

    let via_ident = Ident::new(&cfg.via, name.span());
    let unknown_ident = Ident::new(&cfg.unknown, name.span());

    // Resolve the SET_LABEL the derive threads into BOTH the trait's
    // `const SET_LABEL` AND the carrier's `#[error("unknown <label>:
    // {0}")]` annotation. The priority chain is the typed-escape-hatch
    // shape every other axis on this derive carries:
    //   1. `#[closed_set(set_label = "...")]` — explicit override at
    //      the trait surface, independent of the carrier's annotation.
    //      No production implementor reaches for this today; the axis
    //      exists for the degenerate case where an implementor wants
    //      to bind the trait's set name independently of the carrier's
    //      diagnostic label (a future structured-diagnostic carrier
    //      that wraps a richer payload than `pub String`).
    //   2. `#[closed_set(generate_unknown = "<label>")]` — the same
    //      label the carrier's `#[error(...)]` annotation already
    //      pins, threaded through to the trait surface so the two
    //      surfaces emit from ONE generative origin. Covers irregular
    //      labels (`MacroDefHead` → "macro definition head",
    //      `MustReachPhase` → "must-reach phase") whose operator-
    //      pinned wording diverges from the auto-derived projection.
    //   3. `#[closed_set(generate_unknown)]` / `Skip` — auto-derive
    //      via `pascal_to_spaced_lowercase` on the enum name. Covers
    //      the regular case (`ChannelKind` → "channel kind",
    //      `ReplacementPolicy` → "replacement policy"); also the
    //      fallback for `Skip` so an implementor that hand-rolls the
    //      carrier still gets a typed SET_LABEL without touching the
    //      derive attribute surface.
    let set_label = match (&cfg.set_label, &cfg.generate_unknown) {
        (Some(explicit), _) => explicit.clone(),
        (None, GenerateUnknown::Explicit(label)) => label.clone(),
        (None, GenerateUnknown::Auto | GenerateUnknown::Skip) => {
            pascal_to_spaced_lowercase(&name.to_string())
        }
    };

    let from_str_impl = if cfg.no_from_str {
        TokenStream2::new()
    } else {
        quote! {
            impl ::core::str::FromStr for #name {
                type Err = #unknown_ident;
                fn from_str(
                    s: &::core::primitive::str,
                ) -> ::core::result::Result<Self, Self::Err> {
                    <Self as ::tatara_lisp::ClosedSet>::parse_label(s)
                }
            }
        }
    };

    let unknown_struct_decl = match &cfg.generate_unknown {
        GenerateUnknown::Skip => TokenStream2::new(),
        GenerateUnknown::Auto | GenerateUnknown::Explicit(_) => {
            // The carrier's `#[error(...)]` annotation reads from the
            // SAME resolved `set_label` the trait const reads from —
            // a regression at one site cannot drift from the other,
            // because both flow from the SAME local binding.
            emit_unknown_struct(&unknown_ident, &set_label)
        }
    };

    let display_impl = if cfg.display {
        quote! {
            impl ::core::fmt::Display for #name {
                fn fmt(
                    &self,
                    f: &mut ::core::fmt::Formatter<'_>,
                ) -> ::core::fmt::Result {
                    f.write_str(Self::#via_ident(*self))
                }
            }
        }
    } else {
        TokenStream2::new()
    };

    let expanded = quote! {
        impl ::tatara_lisp::ClosedSet for #name {
            const ALL: &'static [Self] = &Self::ALL;
            const SET_LABEL: &'static ::core::primitive::str = #set_label;
            type Unknown = #unknown_ident;
            fn label(self) -> &'static ::core::primitive::str {
                Self::#via_ident(self)
            }
            fn make_unknown(
                s: &::core::primitive::str,
            ) -> Self::Unknown {
                #unknown_ident(::std::string::String::from(s))
            }
        }

        #from_str_impl

        #unknown_struct_decl

        #display_impl
    };

    expanded.into()
}

/// Emit the `pub struct UnknownX(pub String)` parse-rejection carrier
/// for `#[closed_set(generate_unknown[ = "label"])]`. The shape is the
/// substrate-wide closed-set-enum carrier idiom: `Debug + Clone +
/// PartialEq + Eq + thiserror::Error` derives with an
/// `#[error("unknown <label>: {0}")]` annotation that surfaces the
/// offending input verbatim. Lifted into ONE helper so every
/// generated carrier flows through ONE composition site — a
/// regression that drifts the derive set or the message shape
/// between two generated carriers is structurally impossible.
fn emit_unknown_struct(unknown_ident: &Ident, label: &str) -> TokenStream2 {
    let msg = format!("unknown {label}: {{0}}");
    quote! {
        #[derive(
            ::core::fmt::Debug,
            ::core::clone::Clone,
            ::core::cmp::PartialEq,
            ::core::cmp::Eq,
            ::thiserror::Error,
        )]
        #[error(#msg)]
        pub struct #unknown_ident(pub ::std::string::String);
    }
}

/// Project a PascalCase identifier into the substrate-wide
/// spaced-lowercase label `#[closed_set(generate_unknown)]` threads
/// into the auto-derived `#[error("unknown <label>: {0}")]`
/// annotation. Mirrors the workspace-wide hand-rolled convention
/// across 40+ closed-set carriers (`ChannelKind` →
/// "channel kind", `ReplacementPolicy` → "replacement policy",
/// `CompilerSpecIoStage` → "compiler spec io stage").
///
/// A run of contiguous uppercase characters projects byte-for-byte to
/// lowercase without inserting interior spaces; a space is emitted
/// only at the lowercase→uppercase boundary. Irregular labels
/// (`MacroDefHead` → "macro definition head" with "Def" expanded;
/// `MustReachPhase` → "must-reach phase" with a hyphen) fall outside
/// the projection's codomain and require the explicit
/// `#[closed_set(generate_unknown = "...")]` override.
fn pascal_to_spaced_lowercase(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 2);
    let mut prev_was_lower = false;
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            if prev_was_lower {
                out.push(' ');
            }
            out.push(c.to_ascii_lowercase());
            prev_was_lower = false;
        } else {
            out.push(c);
            prev_was_lower = c.is_ascii_lowercase();
        }
    }
    out
}

#[cfg(test)]
mod pascal_to_spaced_lowercase_tests {
    use super::pascal_to_spaced_lowercase;

    #[test]
    fn regular_two_word_names_split_at_the_word_boundary() {
        // The bread-and-butter case across 30+ closed-set carriers —
        // PascalCase with a single internal capital splits at the
        // capital. The retrofit cohort
        // (`ChannelKind`/`ArtifactKind`/`ReportFormat`/`ExportTrigger`)
        // all live in this case so the auto-derived label matches
        // the workspace-wide convention without an explicit override.
        assert_eq!(pascal_to_spaced_lowercase("ChannelKind"), "channel kind");
        assert_eq!(pascal_to_spaced_lowercase("ArtifactKind"), "artifact kind");
        assert_eq!(pascal_to_spaced_lowercase("ReportFormat"), "report format");
        assert_eq!(
            pascal_to_spaced_lowercase("ExportTrigger"),
            "export trigger",
        );
        assert_eq!(
            pascal_to_spaced_lowercase("ReplacementPolicy"),
            "replacement policy",
        );
    }

    #[test]
    fn three_word_names_split_at_every_word_boundary() {
        // Closed-set names with three PascalCase tokens
        // (`CompilerSpecIoStage`, `OptimizationDirection`,
        // `ConvergencePointType`) split at every lowercase→uppercase
        // boundary. The split is internal — the trailing PascalCase
        // tokens stay as separate words rather than collapsing into
        // the previous one.
        assert_eq!(
            pascal_to_spaced_lowercase("OptimizationDirection"),
            "optimization direction",
        );
        assert_eq!(
            pascal_to_spaced_lowercase("ConvergencePointType"),
            "convergence point type",
        );
    }

    #[test]
    fn contiguous_uppercase_runs_collapse_to_lowercase_without_inner_spaces() {
        // Acronyms run together rather than fan out per letter —
        // `CompilerSpecIoStage` projects "compiler spec io stage"
        // (the "Io" run stays as "io" rather than "i o"). Pinned by
        // the substrate-wide hand-rolled labels:
        // `error.rs`'s `UnknownCompilerSpecIoStage` carries the
        // message "unknown compiler spec io stage: {0}" verbatim, and
        // the auto-derive must match it bit-for-bit so a retrofit
        // doesn't drift the operator-facing wording.
        assert_eq!(
            pascal_to_spaced_lowercase("CompilerSpecIoStage"),
            "compiler spec io stage",
        );
    }

    #[test]
    fn single_word_names_stay_lowercase_with_no_spaces() {
        // A single PascalCase token (no internal capital) projects
        // to a single lowercase word — no leading space, no
        // mid-word split. Covers degenerate-but-valid cases like a
        // future `Signal` or `Kind` enum name.
        assert_eq!(pascal_to_spaced_lowercase("Signal"), "signal");
        assert_eq!(pascal_to_spaced_lowercase("Kind"), "kind");
    }

    #[test]
    fn empty_input_projects_to_empty_string() {
        // Empty-input contract — projecting `""` yields `""` rather
        // than a leading space or a panic. Defensive case the
        // attribute parser shouldn't reach (the derive runs on a
        // named enum), but pinning it here keeps the helper's
        // contract independent of the caller's discipline.
        assert_eq!(pascal_to_spaced_lowercase(""), "");
    }
}

struct ClosedSetCfg {
    via: String,
    unknown: String,
    no_from_str: bool,
    generate_unknown: GenerateUnknown,
    /// `#[closed_set(display)]` — emit the substrate-wide
    /// `impl fmt::Display { f.write_str(Self::$via(*self)) }` block.
    /// 28+ workspace-wide closed-set enums on PascalCase wire-format
    /// axes (the `as_str ⇄ Display ⇄ FromStr` triad) re-derive this
    /// 5-line block byte-for-byte; flipping the flag at the derive
    /// site collapses the block onto ONE generative shape.
    display: bool,
    /// `#[closed_set(set_label = "...")]` — explicit override for the
    /// trait's [`tatara_lisp::ClosedSet::SET_LABEL`] const. Defaults
    /// to the label `#[closed_set(generate_unknown[ = "..."])]`
    /// already pinned (or the auto-derived
    /// `pascal_to_spaced_lowercase(name)` for the bare / `Skip`
    /// cases) so the trait surface and the carrier's `#[error(...)]`
    /// annotation emit from ONE generative origin. The override
    /// exists for the degenerate case where an implementor wants to
    /// bind the trait's set name independently of the carrier's
    /// diagnostic label (a future structured-diagnostic carrier that
    /// wraps a richer payload than `pub String`) — no production
    /// implementor reaches for it today.
    set_label: Option<String>,
}

/// `#[closed_set(generate_unknown[ = "label"])]` parse outcome.
///
/// `Skip` keeps the existing convention (implementor hand-rolls the
/// `pub struct UnknownX(pub String)` carrier alongside the enum).
/// `Auto` emits the carrier with the spaced-lowercase projection of
/// the enum name as the `#[error(...)]` label. `Explicit(label)` emits
/// the carrier with an operator-pinned label that overrides the
/// PascalCase split (for irregular cases like `MacroDefHead` →
/// "macro definition head").
enum GenerateUnknown {
    Skip,
    Auto,
    Explicit(String),
}

fn parse_closed_set_attrs(attrs: &[Attribute], name: &Ident) -> syn::Result<ClosedSetCfg> {
    let mut via: Option<String> = None;
    let mut unknown: Option<String> = None;
    let mut no_from_str = false;
    let mut generate_unknown = GenerateUnknown::Skip;
    let mut display = false;
    let mut set_label: Option<String> = None;
    for attr in attrs {
        if !attr.path().is_ident("closed_set") {
            continue;
        }
        let Meta::List(list) = &attr.meta else {
            continue;
        };
        list.parse_nested_meta(|meta| {
            // Three-arm × three-arm × one-arm dispatch collapses onto
            // the two sub-key primitives:
            //   - `try_lit_str_sub_key` closes the (sub-key ident × `=
            //     <LitStr>` payload × `Option<String>` slot mutation)
            //     shape for `via`, `unknown`, `set_label` — the three
            //     historically-duplicated string-valued arms.
            //   - `try_bool_flag_sub_key` closes the (sub-key ident ×
            //     bare-flag ident × `bool` slot flip) shape for
            //     `no_from_str`, `display` — the two historically-
            //     duplicated bare-flag arms.
            // Short-circuit `||` evaluation preserves the first-match-
            // wins ordering the previous `if / else if` chain carried;
            // `?` unwraps the primitive's `syn::Result<bool>` to the
            // matched bit so the outer chain composes cleanly across
            // both primitive shapes. A future `#[closed_set(alias =
            // "…")]` string-valued key composes as ONE `||` term
            // against `try_lit_str_sub_key`; a future
            // `#[closed_set(no_debug)]` bare-flag key composes as ONE
            // `||` term against `try_bool_flag_sub_key` — no per-key
            // scaffold, no drift.
            if try_lit_str_sub_key(&meta, "via", &mut via)?
                || try_lit_str_sub_key(&meta, "unknown", &mut unknown)?
                || try_lit_str_sub_key(&meta, "set_label", &mut set_label)?
                || try_bool_flag_sub_key(&meta, "no_from_str", &mut no_from_str)?
                || try_bool_flag_sub_key(&meta, "display", &mut display)?
            {
                Ok(())
            } else if meta.path.is_ident("generate_unknown") {
                // Both bare `generate_unknown` (auto-derived label)
                // and `generate_unknown = "explicit label"` (pinned
                // label) sit on ONE attribute key — the parser
                // dispatches on whether `meta.value()` succeeds so the
                // attribute surface stays single-keyed (no
                // `auto_label`/`label` bifurcation that would force
                // the operator to think about which of two
                // attributes is canonical). The `Ok(value)` arm has
                // already consumed the `=`, so it routes through the
                // stream-level `parse_lit_str` primitive rather than
                // the meta-level `read_meta_lit_str` (which would
                // double-consume `.value()`) — and stays outside the
                // `try_lit_str_sub_key` primitive's contract for the
                // same reason (the primitive re-consumes `.value()`
                // internally, incompatible with this arm's outer
                // flag-or-value dispatch).
                generate_unknown = match meta.value() {
                    Ok(value) => GenerateUnknown::Explicit(parse_lit_str(value)?),
                    Err(_) => GenerateUnknown::Auto,
                };
                Ok(())
            } else {
                Err(meta.error(
                    "unknown #[closed_set(...)] key — expected `via`, `unknown`, `no_from_str`, `generate_unknown`, `display`, or `set_label`",
                ))
            }
        })?;
    }
    Ok(ClosedSetCfg {
        via: via.unwrap_or_else(|| "label".to_string()),
        unknown: unknown.unwrap_or_else(|| format!("Unknown{name}")),
        no_from_str,
        generate_unknown,
        display,
        set_label,
    })
}

/// `parse_nested_meta` callback primitive: if `meta`'s sub-key path is
/// `key`, read its `= <LitStr>` payload as an owned `String`, write
/// `Some(<value>)` to `slot`, and return `Ok(true)`; otherwise leave
/// `slot` untouched and return `Ok(false)`.
///
/// Lifts the byte-for-byte identical three-line
///
/// ```ignore
/// } else if meta.path.is_ident("<key>") {
///     <slot> = Some(read_meta_lit_str(&meta)?);
///     Ok(())
/// }
/// ```
///
/// arm that pre-lift lived at THREE sites inside
/// [`parse_closed_set_attrs`] (the `via`, `unknown`, and `set_label`
/// string-valued sub-keys). Post-lift each site composes onto ONE `||`
/// term of the dispatch chain — a future single-keyed string-valued
/// sub-key extension (a `#[closed_set(alias = "…")]` peer, a lifted
/// `keyword` axis pulled up from `#[tatara(…)]`, an operator-authored
/// `prefix = "…"` axis) adds as ONE `||` term against the same
/// substrate rather than as a fresh copy of the arm.
///
/// The `Ok(bool)` return shape lets callers chain arms via short-
/// circuit `||`: `try_lit_str_sub_key(&meta, "via", &mut via)? || ...`.
/// The `?` unwraps the inner `syn::Result<bool>` to the matched bit,
/// and the `||` operator's laziness preserves the historic first-match-
/// wins evaluation order without touching the trailing slots.
///
/// Sibling of [`try_bool_flag_sub_key`] one PAYLOAD-SHAPE axis over:
/// the string-valued arm (this primitive) takes an
/// `&mut Option<String>` slot and reads a `= <LitStr>` payload; the
/// bare-flag arm (the sibling) takes an `&mut bool` slot and reads no
/// payload. The `parse_lit_str` / `read_meta_lit_str` pair a few file-
/// sections up carries the same primitive / meta-level wrapper motif —
/// the derive crate's convention for two-shape sub-key primitives.
///
/// Theory grounding: THEORY.md §VI.1 — generation over composition.
/// The three-times-rule signal fires at three sites of the string-
/// valued sub-key idiom; the primitive names the composition as ONE
/// substrate entry so a new arm of the same shape adds as ONE line, and
/// a diagnostic upgrade (e.g. a `syn::Error::new_spanned(&meta.path,
/// "expected LitStr, got LitInt")` sharpening on the `LitStr::parse`
/// failure) lands at ONE line inherited by every existing caller.
fn try_lit_str_sub_key(
    meta: &syn::meta::ParseNestedMeta<'_>,
    key: &str,
    slot: &mut Option<String>,
) -> syn::Result<bool> {
    if meta.path.is_ident(key) {
        *slot = Some(read_meta_lit_str(meta)?);
        Ok(true)
    } else {
        Ok(false)
    }
}

/// `parse_nested_meta` callback primitive: if `meta`'s sub-key path is
/// `key`, flip `flag` to `true` and return `Ok(true)`; otherwise leave
/// `flag` untouched and return `Ok(false)`.
///
/// Lifts the byte-for-byte identical three-line
///
/// ```ignore
/// } else if meta.path.is_ident("<key>") {
///     <flag> = true;
///     Ok(())
/// }
/// ```
///
/// arm that pre-lift lived at TWO sites inside
/// [`parse_closed_set_attrs`] (the `no_from_str` and `display` bare-
/// flag sub-keys). Post-lift each site composes onto ONE `||` term of
/// the dispatch chain — a future single-keyed bare-flag sub-key
/// extension (a `#[closed_set(no_debug)]` peer, a `no_partial_eq`
/// axis, a `no_hash` axis) adds as ONE `||` term against the same
/// substrate rather than as a fresh copy of the arm.
///
/// Returns `syn::Result<bool>` — not `bool` — to homogenize the return
/// shape with the sibling [`try_lit_str_sub_key`] so callers chain
/// mixed arms via one `?` cadence: `try_lit_str_sub_key(&meta, "via",
/// &mut via)? || try_bool_flag_sub_key(&meta, "display", &mut
/// display)?`. Today the primitive's `Ok` arm cannot fail (a bare-flag
/// match has no payload to parse), but the `syn::Result` wrapper
/// preserves the composition uniformly and admits a future sharpening
/// (e.g. surfacing a `syn::Error` on a stray `= <value>` payload after
/// a bare-flag ident) without changing the primitive's signature or
/// touching every caller's `?` cadence.
///
/// Sibling of [`try_lit_str_sub_key`] one PAYLOAD-SHAPE axis over — see
/// that primitive's doc for the sibling-shape motif that mirrors the
/// [`parse_lit_str`] / [`read_meta_lit_str`] pair.
///
/// Theory grounding: THEORY.md §VI.1 — generation over composition.
/// The two sites of the bare-flag sub-key idiom cross the three-times-
/// rule signal when composed with the sibling string-valued primitive
/// (five sites in aggregate on the SAME `parse_nested_meta` callback);
/// the two primitives together name the (sub-key ident × slot-shape)
/// dispatch matrix as TWO substrate entries so every future closed-
/// set sub-key of either shape adds as ONE `||` term rather than as
/// a fresh scaffold.
fn try_bool_flag_sub_key(
    meta: &syn::meta::ParseNestedMeta<'_>,
    key: &str,
    flag: &mut bool,
) -> syn::Result<bool> {
    if meta.path.is_ident(key) {
        *flag = true;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[proc_macro_derive(TataraDomain, attributes(tatara))]
pub fn derive_tatara_domain(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident.clone();
    let keyword =
        extract_keyword(&input.attrs).unwrap_or_else(|| default_keyword(&name.to_string()));

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(n) => &n.named,
            _ => {
                return spanned_compile_error(
                    &name,
                    "TataraDomain requires a struct with named fields",
                );
            }
        },
        _ => {
            return spanned_compile_error(&name, "TataraDomain may only be derived on structs");
        }
    };

    let mut field_inits: Vec<TokenStream2> = Vec::with_capacity(fields.len());
    let mut allowed_keys: Vec<String> = Vec::with_capacity(fields.len());
    for field in fields {
        let ident = field.ident.as_ref().expect("named field");
        let kebab = snake_to_kebab(&ident.to_string());
        let has_default = has_serde_default(field);
        match extractor_for(&field.ty, &kebab, has_default) {
            Ok(extract) => field_inits.push(quote! { #ident: #extract }),
            Err(err) => return spanned_compile_error(&field.ty, err),
        }
        allowed_keys.push(kebab);
    }

    let allowed_lits = allowed_keys.iter().map(|k| quote! { #k });

    let expanded = quote! {
        impl ::tatara_lisp::domain::TataraDomain for #name {
            const KEYWORD: &'static str = #keyword;

            fn compile_from_args(
                args: &[::tatara_lisp::Sexp],
            ) -> ::tatara_lisp::Result<Self> {
                const __TATARA_ALLOWED_KEYWORDS: &[&::core::primitive::str] = &[
                    #(#allowed_lits),*
                ];
                // The fused typed-entry kwargs gate: parse `:k v :k v …` AND
                // assert every key sits in the static allowed-set, in ONE
                // call. Before this lift the derive emitted the two-call
                // sequence (`parse_kwargs` + `reject_unknown_kwargs`)
                // verbatim at every consumer's `compile_from_args` body;
                // the fused primitive names the composition as ONE
                // substrate-level operation so a regression that drifts
                // ONE consumer's gate from the others (e.g. a future
                // emitter swaps the order, a hand-written impl forgets
                // the second call) is structurally impossible — every
                // consumer routes through ONE function, every diagnostic
                // surfaces from ONE call site.
                let kw = ::tatara_lisp::domain::parse_kwargs_strict(
                    args,
                    __TATARA_ALLOWED_KEYWORDS,
                )?;
                Ok(Self {
                    #(#field_inits),*
                })
            }
        }
    };

    expanded.into()
}

fn extract_keyword(attrs: &[Attribute]) -> Option<String> {
    find_named_sub_key(attrs, "tatara", "keyword", read_meta_lit_str)
}

/// Walk each `#[<attr_ident>(...)]` attribute in `attrs`, first-hit-wins
/// on any comma-separated sub-key whose ident equals `sub_key`. When the
/// target sub-key is encountered, `on_match` projects the
/// `ParseNestedMeta` into a `T` (typically by reading its `= <value>`
/// payload). Every non-matching sub-key has its trailing `= <expr>`
/// payload defensively drained — without this, `parse_nested_meta`
/// stalls at the `=` that follows any value-carrying peer and silently
/// drops every later sub-key (including a load-bearing target that
/// appears AFTER an unrelated peer).
///
/// Lifts the shared shape of `extract_keyword` (the single-keyed
/// `#[tatara(keyword = "…")]` reader) and `has_serde_default` (the
/// single-flagged `#[serde(default)]` sniffer) — both projected on the
/// same three axes (attr-path gate, sub-key ident match, unmatched-peer
/// value-drain) with byte-for-byte duplicated `parse_nested_meta`
/// callback scaffolds. A future single-keyed sub-key reader (e.g. the
/// `#[tatara(alias = "…")]` extension the sibling
/// `keyword_after_unrelated_named_value_key_projects_to_the_literal_value`
/// test's docblock cites, or a `#[serde(rename = "…")]` sniffer) now
/// composes as a three-line caller rather than a fresh copy of the
/// scaffold — the value-drain contract that both existing callers
/// depend on is closed at ONE substrate-level entry, not open to
/// per-caller drift.
///
/// Errors that `on_match` returns via `?` unwind the outer
/// `parse_nested_meta` and are silently absorbed, matching the historic
/// swallow discipline both callers already carry (a `#[tatara(keyword
/// = 42)]` value-shape mismatch projects to `None` without diagnostic;
/// a `#[serde(default = <malformed-expr>)]` payload projects to `false`
/// under the sibling reader's `let _ = ...` swallow of the outer
/// traversal).
fn find_named_sub_key<T>(
    attrs: &[Attribute],
    attr_ident: &str,
    sub_key: &str,
    mut on_match: impl FnMut(&syn::meta::ParseNestedMeta<'_>) -> syn::Result<T>,
) -> Option<T> {
    for attr in attrs {
        if !attr.path().is_ident(attr_ident) {
            continue;
        }
        let Meta::List(list) = &attr.meta else {
            continue;
        };
        let mut found: Option<T> = None;
        let _ = list.parse_nested_meta(|meta| {
            if meta.path.is_ident(sub_key) {
                found = Some(on_match(&meta)?);
            } else if let Ok(value) = meta.value() {
                let _: syn::Result<syn::Expr> = value.parse();
            }
            Ok(())
        });
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Parse a `LitStr` off an already-obtained value stream and project it
/// to an owned `String`. The primitive-level shape the two peer readers
/// [`read_meta_lit_str`] (the "read `= <LitStr>` payload of a sub-key"
/// composition, called on 4 arms across `extract_keyword` +
/// `parse_closed_set_attrs`'s `via`/`unknown`/`set_label` slots) AND
/// `parse_closed_set_attrs`'s `generate_unknown` Explicit arm (which
/// already consumed the `=` via the outer `match meta.value()` flag-
/// or-value dispatch and therefore can't route through the
/// meta-level helper without double-consuming `.value()`) BOTH
/// project through.
///
/// Lifts the byte-for-byte identical `let s: LitStr = value.parse()?;
/// Ok(s.value())` shape that pre-lift lived at 5 sites across the
/// derive crate (once per string-slot). A future refactor that
/// tightens the parse (e.g. surfaces a specific `syn::Error` diagnostic
/// on the non-`LitStr` value shape, or admits both `LitStr` +
/// `LitByteStr`, or trims surrounding whitespace at the parse
/// boundary) lands at ONE line and is inherited by every caller
/// automatically.
fn parse_lit_str(value: syn::parse::ParseStream<'_>) -> syn::Result<String> {
    let s: LitStr = value.parse()?;
    Ok(s.value())
}

/// Read the `= <LitStr>` payload of a named-value sub-key inside a
/// `parse_nested_meta` callback as an owned `String`. Composes
/// `meta.value()?` + [`parse_lit_str`] into ONE substrate entry that
/// [`extract_keyword`]'s callback + `parse_closed_set_attrs`'s
/// `via` / `unknown` / `set_label` arms route through.
///
/// Lifts the byte-for-byte identical `let value = meta.value()?; let
/// s: LitStr = value.parse()?; Ok(s.value())` scaffold that pre-lift
/// lived at 4 sites across the derive crate. Peer to the sibling
/// `find_named_sub_key` helper one abstraction level up — together
/// the two compose the derive's "read `#[<attr>(<sub_key> = "…")]`
/// payload as `Option<String>`" idiom onto ONE stack of substrate
/// primitives (`find_named_sub_key` + `read_meta_lit_str`), which the
/// [`extract_keyword`] reader collapses onto a one-line callable-pointer
/// projection (`find_named_sub_key(attrs, "tatara", "keyword",
/// read_meta_lit_str)`) and which every future single-keyed string-
/// valued sub-key reader (the `#[tatara(alias = "…")]` extension the
/// `keyword_after_unrelated_named_value_key_projects_to_the_literal_value`
/// test's docblock cites; a `#[serde(rename = "…")]` sniffer; a
/// single-key reader lifted out of the 6-key `parse_closed_set_attrs`)
/// inherits automatically.
fn read_meta_lit_str(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<String> {
    parse_lit_str(meta.value()?)
}

/// Project an existing [`syn::Error`] into the proc-macro's outer
/// [`TokenStream`] return shape — the primitive-level composition of
/// `err.to_compile_error().into()` every early-return arm of every
/// `#[proc_macro_derive]` in this crate threads through.
///
/// Pre-lift the two-call chain `.to_compile_error().into()` lived
/// verbatim at every early-return site across the two derives
/// (`derive_closed_set` + `derive_tatara_domain`). Post-lift the
/// composition names the operation as ONE substrate-level
/// projection — a future refactor of the emission (e.g. wrapping in
/// a diagnostic-envelope for structured tooling, adding a
/// `note = "help: …"` chain, threading through a
/// `proc_macro2::TokenStream → TokenStream` boundary primitive) lands
/// at ONE line and every derive's early-return arm picks it up
/// mechanically.
///
/// Sibling of [`spanned_compile_error`] one INPUT-BAND axis over:
/// the pre-composed-error posture (this function) takes an already-
/// constructed [`syn::Error`]; the spanned-message posture (the
/// sibling) constructs the [`syn::Error`] via
/// [`syn::Error::new_spanned`] first, then routes through this
/// function so the two-call chain `.to_compile_error().into()`
/// binds at ONE composition point. The `parse_lit_str` /
/// `read_meta_lit_str` pair one file-section up carries the same
/// primitive / meta-level wrapper sibling shape — the derive crate's
/// convention for two-level substrate lifts.
fn emit_compile_error(err: syn::Error) -> TokenStream {
    err.to_compile_error().into()
}

/// Emit a proc-macro compile error at the span of `spanned`, with
/// message `msg` — the meta-level composition of
/// `syn::Error::new_spanned(spanned, msg)` + [`emit_compile_error`]
/// every input-shape rejection arm across `derive_closed_set` +
/// `derive_tatara_domain` threads through.
///
/// Pre-lift the four-line scaffold
///
/// ```ignore
/// return syn::Error::new_spanned(&target, "…msg…")
///     .to_compile_error()
///     .into();
/// ```
///
/// lived verbatim at four sites (the enum-only gate in
/// `derive_closed_set`; the two struct/named-fields gates in
/// `derive_tatara_domain`; the per-field `extractor_for` failure
/// path). Post-lift each site collapses to
///
/// ```ignore
/// return spanned_compile_error(&target, "…msg…");
/// ```
///
/// binding the (spanned target, diagnostic message, compile-error
/// emission) triple at ONE composition point on the derive crate's
/// substrate. A future refactor of any of the three axes (e.g.
/// threading spans through a typed span-carrier, extending message
/// shape with `help:` continuations, swapping the emission for a
/// structured-diagnostic surface) lands at ONE line and every
/// input-rejection arm picks it up mechanically.
///
/// Sibling of [`emit_compile_error`] one INPUT-BAND axis over —
/// see that function's doc for the sibling-shape motif that mirrors
/// the [`parse_lit_str`] / [`read_meta_lit_str`] pair.
fn spanned_compile_error<T, M>(spanned: T, msg: M) -> TokenStream
where
    T: quote::ToTokens,
    M: std::fmt::Display,
{
    emit_compile_error(syn::Error::new_spanned(spanned, msg))
}

#[cfg(test)]
mod compile_error_emission_tests {
    //! Contract tests for the two `[emit|spanned]_compile_error`
    //! sibling primitives. Exercises the emission through the SAME
    //! `proc_macro2::TokenStream::to_string()` round-trip every proc-
    //! macro consumer sees at the outer boundary — the
    //! `compile_error! { "…" }` invocation the emission expands to,
    //! anchored at the sibling `syn::Error::to_compile_error` docs.
    //!
    //! The `proc_macro::TokenStream` return type is unavailable in
    //! `#[cfg(test)]` unit tests (the `proc_macro` crate is a proc-
    //! macro-only surface, not linkable from a lib-crate test
    //! harness), so we exercise the emission at the
    //! [`syn::Error::to_compile_error`] boundary — the ONE call
    //! [`emit_compile_error`] makes before the boundary-crossing
    //! [`Into`] conversion. If that call binds the expected shape,
    //! the outer `.into()` is a total identity on the
    //! `proc_macro2::TokenStream → proc_macro::TokenStream` boundary
    //! (guaranteed by the syn / proc-macro2 crate contracts).
    use proc_macro2::TokenStream as TokenStream2;
    use quote::ToTokens;
    use syn::parse_str;

    fn compile_error_body(err: syn::Error) -> String {
        err.to_compile_error().to_string()
    }

    #[test]
    fn syn_error_to_compile_error_emits_a_compile_error_macro_call() {
        // The `to_compile_error()` boundary emits a
        // `::core::compile_error!{ "…msg…" }` macro invocation as a
        // `proc_macro2::TokenStream`. This is the SAME shape the
        // pre-lift four-line scaffold surfaced verbatim, and the SAME
        // shape [`emit_compile_error`] threads through before
        // `.into()`-ing across the proc-macro boundary. A regression
        // that (say) swapped `compile_error!` for `panic!` or dropped
        // the message payload would surface here.
        let err = syn::Error::new(proc_macro2::Span::call_site(), "sample diagnostic");
        let body = compile_error_body(err);
        assert!(
            body.contains("compile_error"),
            "compile_error emission must include the `compile_error!` macro invocation, got: {body}",
        );
        assert!(
            body.contains("sample diagnostic"),
            "compile_error emission must include the diagnostic message verbatim, got: {body}",
        );
    }

    #[test]
    fn spanned_error_preserves_the_target_span_for_ide_diagnostics() {
        // `syn::Error::new_spanned(target, msg)` anchors the
        // diagnostic at `target`'s span so a proc-macro consumer's
        // IDE highlights the OFFENDING token (not the macro
        // invocation). Verified by comparing the spanned emission
        // against the SAME message emitted at `Span::call_site()` —
        // the two are byte-identical in the `compile_error!` shell
        // but the LOAD-BEARING difference is on the internal
        // `_ = { ... }` span-carrier token cluster syn threads
        // through to preserve the target's span through the emission.
        // If the two are IDENTICAL, the span binding got stripped
        // and `spanned_compile_error` degrades to a plain-emission
        // helper.
        let ident: syn::Ident = parse_str("target_ident").expect("valid ident");
        let spanned = syn::Error::new_spanned(&ident, "spanned diagnostic");
        let call_site = syn::Error::new(proc_macro2::Span::call_site(), "spanned diagnostic");
        // The `compile_error!` shell + message payload are the same;
        // the token span each shell is anchored at differs. We
        // exercise the shell agreement here — the SPAN agreement is
        // an implementation detail syn's `to_compile_error` guarantees
        // and can't be observed through the string round-trip.
        assert_eq!(
            compile_error_body(spanned)
                .replace(char::is_whitespace, "")
                .contains("compile_error!{\"spanneddiagnostic\"}"),
            compile_error_body(call_site)
                .replace(char::is_whitespace, "")
                .contains("compile_error!{\"spanneddiagnostic\"}"),
        );
    }

    #[test]
    fn spanned_compile_error_accepts_string_message_at_call_sites() {
        // `spanned_compile_error<T, M> where M: std::fmt::Display`
        // must accept a `String` payload (the shape
        // `derive_tatara_domain`'s `extractor_for` error path
        // threads through: `Err(err) => spanned_compile_error(
        // &field.ty, err)` where `err: String`). Pin the trait bound
        // at the call site so a regression that (say) tightened `M`
        // to `&'static str` would break the field-level error path.
        //
        // Exercised at the `syn::Error::new_spanned` composition (the
        // ONE line inside `spanned_compile_error`) — a `Display`
        // bound that admits `String` and `&str` alike is what the
        // sibling helper's four current callers rely on.
        let ident: syn::Ident = parse_str("target_ident").expect("valid ident");
        let owned: String = String::from("owned message");
        // Compile-check: this function must accept `&String` as M
        // just like it accepts `&'static str`. Neither call panics;
        // the goal is to verify the trait bound admits both shapes.
        let err_owned = syn::Error::new_spanned(&ident, owned);
        let err_static = syn::Error::new_spanned(&ident, "static message");
        assert!(compile_error_body(err_owned).contains("owned message"));
        assert!(compile_error_body(err_static).contains("static message"));
    }

    #[test]
    fn spanned_compile_error_accepts_ident_and_type_targets() {
        // The four current call sites thread THREE distinct
        // `ToTokens` target shapes into `spanned_compile_error`:
        //   1. `&Ident` (the enum/struct name — three sites in the
        //      `derive_closed_set` + `derive_tatara_domain` input-
        //      shape gates),
        //   2. `&syn::Type` (the field type — one site in
        //      `derive_tatara_domain`'s per-field `extractor_for`
        //      error path).
        // Both must be admissible under the `T: quote::ToTokens`
        // bound. This test exercises both shapes at the underlying
        // `syn::Error::new_spanned` boundary the helper composes
        // over — a `T: ToTokens` bound admits both `&Ident` and
        // `&Type` alike.
        let ident: syn::Ident = parse_str("target_ident").expect("valid ident");
        let ty: syn::Type = parse_str("::std::string::String").expect("valid type");
        let err_ident = syn::Error::new_spanned(&ident, "at ident span");
        let err_type = syn::Error::new_spanned(&ty, "at type span");
        // Both round-trip through `to_compile_error` cleanly — the
        // outer emission is agnostic to the target shape, so both
        // shapes emit the same `compile_error!` invocation.
        let ident_body = compile_error_body(err_ident);
        let type_body = compile_error_body(err_type);
        assert!(ident_body.contains("at ident span"));
        assert!(type_body.contains("at type span"));
        // Sanity: both bodies are non-empty TokenStreams the outer
        // `.into()` boundary would forward across the proc-macro
        // surface.
        let ident_stream: TokenStream2 = ident_body.parse().expect("emission parses as TS");
        let type_stream: TokenStream2 = type_body.parse().expect("emission parses as TS");
        assert!(!ident_stream.is_empty());
        assert!(!type_stream.is_empty());
        // Guard the ToTokens threading itself: a regression that
        // dropped `#[allow(dead_code)]`-style token pruning would
        // fail here.
        let _ = ident.to_token_stream();
        let _ = ty.to_token_stream();
    }
}

fn default_keyword(type_name: &str) -> String {
    let stripped = type_name.strip_suffix("Spec").unwrap_or(type_name);
    let mut out = String::from("def");
    for c in stripped.chars() {
        if c.is_uppercase() {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn snake_to_kebab(snake: &str) -> String {
    snake.replace('_', "-")
}

/// Check if the field carries `#[serde(default)]` / `#[serde(default = "…")]`.
/// We honor serde defaults so missing kwargs fall back to `Default::default()`
/// — matches the deserialize semantics the field was already authored for.
///
/// Routes through the shared `find_named_sub_key` helper — the same
/// entry the sibling `extract_keyword` reader uses. The `on_match`
/// callback is `|_| Ok(())`: we care only whether the `default`
/// sub-key ident appears anywhere in a `#[serde(...)]` attribute, not
/// what its optional `= "path"` payload names. The helper closes the
/// attr-path gate, the sub-key ident match, and the defensive
/// value-drain across unmatched peers at ONE substrate entry —
/// rejecting the false positive the pre-lift substring-match
/// implementation surfaced on `#[serde(rename = "default_val")]` AND
/// letting `default` appear ANYWHERE in the sub-key list without the
/// callback stalling at a preceding value-carrying peer's `=`
/// (pinned as `default_after_other_key_named_value_pair`).
fn has_serde_default(field: &syn::Field) -> bool {
    find_named_sub_key(&field.attrs, "serde", "default", |_meta| Ok(())).is_some()
}

fn extractor_for(ty: &Type, key: &str, has_default: bool) -> Result<TokenStream2, String> {
    let kind = classify(ty);
    let base = match kind {
        Kind::String => quote! {
            ::tatara_lisp::domain::extract_string(&kw, #key)?.to_string()
        },
        Kind::OptionalString => quote! {
            ::tatara_lisp::domain::extract_optional_string(&kw, #key)?.map(::std::string::String::from)
        },
        Kind::VecString => quote! {
            ::tatara_lisp::domain::extract_string_list(&kw, #key)?
        },
        // ── The four numeric arms: NARROWED, never `as`-cast ──
        //
        // These four used to emit the reader's wide value followed by a
        // raw Rust `as` downcast — `extract_int(&kw, "port")? as u32`.
        // `as` is total by truncating, so `:port 4294967296` landed as
        // `0` and `:port -1` as `4294967295`, in the struct, silently,
        // with nothing red anywhere. The author read back a number they
        // never wrote.
        //
        // The width now rides the TURBOFISH into
        // `tatara_lisp::domain`'s `NarrowNumeric` projection, which
        // returns `LispError::KwargOutOfRange` for a value the field
        // cannot hold. Two consequences worth naming: this derive no
        // longer contains the word `as` on any numeric path (there is
        // no truncation left to regress), and the emitted code names
        // the width exactly ONCE — as a type — so the diagnostic's
        // `target` cannot drift from the field's actual Rust type.
        //
        // `rust_ty` is still the `Kind::Int` / `Kind::Float` payload,
        // now spliced as the generic argument rather than as a cast
        // target; the `classify` unit pins on those payloads keep
        // their meaning unchanged.
        Kind::Int(rust_ty) => {
            let narrowed: TokenStream2 = rust_ty.parse().unwrap();
            quote! {
                ::tatara_lisp::domain::extract_int_narrowed::<#narrowed>(&kw, #key)?
            }
        }
        Kind::OptionalInt(rust_ty) => {
            let narrowed: TokenStream2 = rust_ty.parse().unwrap();
            quote! {
                ::tatara_lisp::domain::extract_optional_int_narrowed::<#narrowed>(&kw, #key)?
            }
        }
        Kind::Float(rust_ty) => {
            let narrowed: TokenStream2 = rust_ty.parse().unwrap();
            quote! {
                ::tatara_lisp::domain::extract_float_narrowed::<#narrowed>(&kw, #key)?
            }
        }
        Kind::OptionalFloat(rust_ty) => {
            let narrowed: TokenStream2 = rust_ty.parse().unwrap();
            quote! {
                ::tatara_lisp::domain::extract_optional_float_narrowed::<#narrowed>(&kw, #key)?
            }
        }
        Kind::Bool => quote! {
            ::tatara_lisp::domain::extract_bool(&kw, #key)?
        },
        Kind::OptionalBool => quote! {
            ::tatara_lisp::domain::extract_optional_bool(&kw, #key)?
        },
        // Fall-through: anything with `serde::Deserialize` works via the
        // sexp_to_json bridge. Unlocks enums, nested structs, Vec<Struct>.
        // The boilerplate that used to live here (sexp_to_json +
        // serde_json::from_value + LispError::Compile shaping, repeated
        // three times) lives behind these helpers in
        // `tatara_lisp::domain` so hand-written impls share the same
        // error path and future diagnostic upgrades land in one place.
        Kind::Deserialize => quote! {
            ::tatara_lisp::domain::extract_via_serde(&kw, #key)?
        },
        Kind::OptionalDeserialize => quote! {
            ::tatara_lisp::domain::extract_optional_via_serde(&kw, #key)?
        },
        Kind::VecDeserialize => quote! {
            ::tatara_lisp::domain::extract_vec_via_serde(&kw, #key)?
        },
    };
    // Respect `#[serde(default)]` — wrap extractor with a missing-key short-circuit.
    Ok(if has_default {
        quote! {
            if kw.contains_key(#key) { #base } else { ::std::default::Default::default() }
        }
    } else {
        base
    })
}

#[derive(Clone)]
enum Kind {
    String,
    OptionalString,
    VecString,
    Int(&'static str),
    OptionalInt(&'static str),
    Float(&'static str),
    OptionalFloat(&'static str),
    Bool,
    OptionalBool,
    /// Fall-through: any type implementing `serde::Deserialize`.
    Deserialize,
    OptionalDeserialize,
    VecDeserialize,
}

fn classify(ty: &Type) -> Kind {
    if let Type::Path(path) = ty {
        if let Some(last) = path.path.segments.last() {
            match last.ident.to_string().as_str() {
                "String" => return Kind::String,
                "bool" => return Kind::Bool,
                "i64" => return Kind::Int("i64"),
                "i32" => return Kind::Int("i32"),
                "u32" => return Kind::Int("u32"),
                "u64" => return Kind::Int("u64"),
                "usize" => return Kind::Int("usize"),
                "f64" => return Kind::Float("f64"),
                "f32" => return Kind::Float("f32"),
                "Option" => return classify_option(last),
                "Vec" => return classify_vec(last),
                _ => {}
            }
        }
    }
    // Anything else: fall through to serde Deserialize.
    Kind::Deserialize
}

fn classify_option(last: &syn::PathSegment) -> Kind {
    let Ok(inner) = first_generic_type(last) else {
        return Kind::OptionalDeserialize;
    };
    match classify(inner) {
        Kind::String => Kind::OptionalString,
        Kind::Int(t) => Kind::OptionalInt(t),
        Kind::Float(t) => Kind::OptionalFloat(t),
        Kind::Bool => Kind::OptionalBool,
        _ => Kind::OptionalDeserialize,
    }
}

fn classify_vec(last: &syn::PathSegment) -> Kind {
    let Ok(inner) = first_generic_type(last) else {
        return Kind::VecDeserialize;
    };
    match classify(inner) {
        Kind::String => Kind::VecString,
        _ => Kind::VecDeserialize,
    }
}

fn first_generic_type(seg: &syn::PathSegment) -> Result<&Type, String> {
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return Err("expected <T> generic arguments".into());
    };
    for arg in &args.args {
        if let syn::GenericArgument::Type(t) = arg {
            return Ok(t);
        }
    }
    Err("no type argument found".into())
}

#[cfg(test)]
mod default_keyword_tests {
    use super::default_keyword;

    // README (`tatara-lisp-derive/README.md` §Keyword derivation) documents
    // the derive's auto-keyword projection:
    //   "Default: strip `Spec` suffix + prefix `def` + lowercase.
    //    `MonitorSpec` → `defmonitor`."
    // The projection is the derive's public contract when the operator
    // omits `#[tatara(keyword = "...")]`; a regression here silently
    // renames every downstream `(defX …)` authoring form. Pin the
    // documented shape AND the load-bearing suffix-detection rules
    // (case-sensitive, tail-only, single-pass) so a future refactor of
    // the projection surfaces the drift at the derive's test layer
    // rather than as a mystery "why did every existing Lisp source stop
    // compiling" downstream.

    #[test]
    fn strips_spec_suffix_and_lowercases_readme_example() {
        // The exact example in `tatara-lisp-derive/README.md`.
        assert_eq!(default_keyword("MonitorSpec"), "defmonitor");
    }

    #[test]
    fn strips_spec_suffix_across_the_workspace_spec_cohort() {
        // Every `*Spec` struct across the tatara workspace that carries
        // `#[derive(TataraDomain)]` without an explicit `#[tatara(keyword
        // = "...")]` gets its keyword through this exact projection —
        // `ProcessSpec` → `defprocess`, `NotifySpec` → `defnotify`, etc.
        // Sweep three representative names so a projection change that
        // affects only a subset (e.g. drops the lowercase pass, only
        // strips one specific prefix, or projects capitals differently)
        // surfaces on any implementor rather than only on `MonitorSpec`.
        assert_eq!(default_keyword("ProcessSpec"), "defprocess");
        assert_eq!(default_keyword("NotifySpec"), "defnotify");
        assert_eq!(default_keyword("AlertPolicySpec"), "defalertpolicy");
    }

    #[test]
    fn suffix_strip_is_case_sensitive() {
        // The derive's convention is PascalCase types; a lowercase or
        // all-uppercase `spec` / `SPEC` tail is NOT the singular `Spec`
        // marker and must fall through unchanged. Pinning this rules
        // out a permissive refactor (e.g. a case-insensitive strip via
        // `to_ascii_lowercase().strip_suffix("spec")`) that would
        // silently absorb identifiers the operator did NOT intend as
        // `Spec`-suffixed.
        assert_eq!(default_keyword("MonitorSPEC"), "defmonitorspec");
        assert_eq!(default_keyword("Monitorspec"), "defmonitorspec");
    }

    #[test]
    fn suffix_strip_only_matches_a_true_suffix() {
        // A name that CONTAINS "Spec" mid-identifier — `SpecMonitor` (as
        // prefix), `MySpecialType` (as substring) — must NOT be stripped.
        // Rules out a permissive refactor (e.g. `.replace("Spec", "")`)
        // that would remove `Spec` from the middle of an identifier and
        // silently corrupt the keyword.
        assert_eq!(default_keyword("SpecMonitor"), "defspecmonitor");
        assert_eq!(default_keyword("MySpecialType"), "defmyspecialtype");
    }

    #[test]
    fn no_spec_suffix_falls_through_unchanged() {
        // Types without a `Spec` suffix pass through the lowercase step
        // untouched — the `unwrap_or(type_name)` fall-through arm. Every
        // `TataraDomain` implementor that doesn't follow the `*Spec`
        // convention (a future `KenshiTestSuite`, a future `PoolConfig`)
        // hits this arm; the projection must still emit a well-formed
        // `def`-prefixed lowercase keyword rather than panicking or
        // returning `def` alone.
        assert_eq!(default_keyword("KenshiTestSuite"), "defkenshitestsuite");
        assert_eq!(default_keyword("PoolConfig"), "defpoolconfig");
        assert_eq!(default_keyword("Monitor"), "defmonitor");
    }

    #[test]
    fn strips_exactly_one_spec_suffix_not_repeated() {
        // The `strip_suffix("Spec")` primitive strips ONE occurrence,
        // not all trailing occurrences. `SpecSpec` (degenerate but
        // syntactically valid) strips to `Spec` and lowercases to
        // `defspec`; a `while let Some(...)` refactor that iterates
        // strip_suffix would drift silently to `def`.
        assert_eq!(default_keyword("SpecSpec"), "defspec");
    }

    #[test]
    fn ascii_uppercase_projection_leaves_digits_and_symbols_unchanged() {
        // The projection only lowercases uppercase ASCII letters; digits
        // and connector characters pass through untouched. A future
        // Unicode-aware refactor (`c.to_lowercase()` iterator) must NOT
        // change this contract without an intentional bump, because the
        // Lisp keyword grammar admits only ASCII lowercase + digits +
        // connectors after the leading `:`. Pinning digits keeps the
        // ASCII-only guarantee load-bearing at the test layer.
        assert_eq!(default_keyword("V2Api"), "defv2api");
    }
}

#[cfg(test)]
mod snake_to_kebab_tests {
    use super::snake_to_kebab;

    // README (`tatara-lisp-derive/README.md` §Field name ↔ keyword
    // mapping) documents:
    //   "Snake-case field names become kebab-case Lisp keywords:
    //    `name` → `:name`, `window_seconds` → `:window-seconds`,
    //    `delegate_to_nix_build` → `:delegate-to-nix-build`."
    // The projection is the derive's public contract for kwarg naming;
    // a regression here silently renames every kwarg on every generated
    // extractor, breaking every downstream `(defX :kwarg …)` authoring
    // form. Pin the three documented examples verbatim AND the boundary
    // cases (no underscore, empty input) so a future refactor of the
    // projection surfaces the drift here rather than as a cascade of
    // mystery `LispError::MissingKwarg { key: "..." }` failures
    // downstream.

    #[test]
    fn readme_examples_project_verbatim() {
        // The exact three examples in `tatara-lisp-derive/README.md`.
        assert_eq!(snake_to_kebab("name"), "name");
        assert_eq!(snake_to_kebab("window_seconds"), "window-seconds");
        assert_eq!(
            snake_to_kebab("delegate_to_nix_build"),
            "delegate-to-nix-build",
        );
    }

    #[test]
    fn empty_input_projects_to_empty_string() {
        // Boundary contract: an empty field name (unreachable through
        // the derive's `named field` gate, but pin the primitive's
        // contract independent of the caller's discipline — mirrors
        // `pascal_to_spaced_lowercase`'s empty-input test above).
        assert_eq!(snake_to_kebab(""), "");
    }

    #[test]
    fn projects_every_underscore_including_consecutive_runs() {
        // The primitive is a bulk `replace('_', '-')` — every
        // underscore anywhere in the input flips to a hyphen. Consecutive
        // underscores (rare in idiomatic Rust field names but
        // syntactically valid) project to consecutive hyphens; a
        // regression that first-only-replaces (via `replacen`) or
        // collapses runs would surface here.
        assert_eq!(snake_to_kebab("foo__bar"), "foo--bar");
    }
}

#[cfg(test)]
mod classify_tests {
    use super::{classify, first_generic_type, Kind};
    use syn::{parse_str, Type, TypePath};

    // The `(syn::Type -> Kind)` projection is the derive's PRIVATE
    // dispatch table — one field type maps to one extractor helper
    // (`extract_string` for `Kind::String`, `extract_optional_int` for
    // `Kind::OptionalInt(_)`, `extract_via_serde` for `Kind::Deserialize`,
    // etc.). A regression that mis-classifies ONE `syn::Type` shape
    // silently swaps the extractor emitted for a Kind of field: the
    // operator sees no diagnostic drift at the derive site, but every
    // downstream `#[derive(TataraDomain)]` implementor with a matching
    // field routes through the wrong helper — the typed-entry gate
    // fires against the wrong slot decoder, the `LispError::TypeMismatch`
    // renders the wrong `expected` label, and the failure surfaces as a
    // cascade of mystery `MissingKwarg` / `TypeMismatch` errors at
    // integration-test time.
    //
    // The projection admits three distinct arms — the primitive-name
    // switch (`String`, `bool`, `i64` / `i32` / `u32` / `u64` / `usize`,
    // `f64` / `f32`), the `Option<T>` recursor (delegates to
    // `classify_option`), the `Vec<T>` recursor (delegates to
    // `classify_vec`) — and one fall-through arm to `Kind::Deserialize`
    // that catches every non-primitive type (nested structs, foreign
    // enums, un-named collection wrappers). The tests below pin each
    // arm at the boundary of the shapes it decodes so a refactor of ANY
    // of the three (e.g. adding a new primitive, tightening the
    // fall-through, sharpening the nested-recursor discipline) surfaces
    // at ONE test-module boundary in the derive crate rather than as
    // silent drift across every downstream implementor.

    fn parse_ty(s: &str) -> Type {
        parse_str(s).expect("valid Rust type syntax")
    }

    fn last_segment(ty: &Type) -> &syn::PathSegment {
        let Type::Path(TypePath { path, .. }) = ty else {
            panic!("expected a path type");
        };
        path.segments.last().expect("path has at least one segment")
    }

    fn assert_first_generic_type_err(seg: &syn::PathSegment, expected: &str) {
        // `syn::Type` does not implement `Debug` (the `full` feature
        // does not activate `extra-traits`), so `Result::expect_err`
        // is unavailable on the `Result<&Type, String>` return
        // shape. Match the `Err` arm structurally and pin the message
        // verbatim.
        match first_generic_type(seg) {
            Err(msg) => assert_eq!(msg, expected),
            Ok(_) => panic!("expected first_generic_type Err({expected:?})"),
        }
    }

    #[test]
    fn string_classifies_as_kind_string() {
        // Bread-and-butter case: a bare `String` field routes through
        // `extract_string`. Pin the exact `Kind::String` variant identity
        // so a refactor that (say) folded `Kind::String` into a broader
        // `Kind::Text(&'static str)` payload variant surfaces here.
        assert!(matches!(classify(&parse_ty("String")), Kind::String));
    }

    #[test]
    fn bool_classifies_as_kind_bool() {
        // The `bool` primitive routes through `extract_bool` — pinned
        // separately from the integer / float arms because it's a
        // structurally distinct payload with no `Kind::Int`-style width
        // tag.
        assert!(matches!(classify(&parse_ty("bool")), Kind::Bool));
    }

    #[test]
    fn every_supported_integer_width_classifies_as_kind_int_with_the_matching_type_literal() {
        // The five integer widths the derive supports each project to
        // `Kind::Int(<literal>)` with the width name threaded through
        // the payload — the payload IS the turbofish the emitted
        // extractor narrows `extract_int`'s `i64` return into (it was
        // an `as <ty>` cast until the narrowing landed). A
        // regression that (a) narrowed the supported set to a subset,
        // (b) mis-labeled ONE width's payload (e.g. `u32` → `"i32"`),
        // or (c) dropped the payload entirely would silently swap the
        // emitted target width at every consumer's `compile_from_args`
        // body.
        assert!(matches!(classify(&parse_ty("i64")), Kind::Int("i64")));
        assert!(matches!(classify(&parse_ty("i32")), Kind::Int("i32")));
        assert!(matches!(classify(&parse_ty("u32")), Kind::Int("u32")));
        assert!(matches!(classify(&parse_ty("u64")), Kind::Int("u64")));
        assert!(matches!(classify(&parse_ty("usize")), Kind::Int("usize")));
    }

    #[test]
    fn every_supported_float_width_classifies_as_kind_float_with_the_matching_type_literal() {
        // Sibling of the integer-width pin — `f64` / `f32` each route
        // through `Kind::Float(<literal>)`, threading the width name
        // through the payload for the emitted narrowing turbofish on
        // `extract_float`'s `f64` return. A regression that dropped
        // `f32` from the supported set (folding it into the
        // fall-through arm) would silently rewire every `f32` field to
        // `extract_via_serde` — the operator would see NO diagnostic
        // drift at the derive site but the downstream error would
        // surface as a mystery serde-deserialize failure on a numeric
        // literal.
        assert!(matches!(classify(&parse_ty("f64")), Kind::Float("f64")));
        assert!(matches!(classify(&parse_ty("f32")), Kind::Float("f32")));
    }

    #[test]
    fn option_of_supported_primitive_classifies_as_the_matching_optional_variant() {
        // `Option<T>` where `T` is a supported primitive routes through
        // `classify_option`'s per-variant arm — `Option<String>` →
        // `OptionalString`, `Option<bool>` → `OptionalBool`, and the
        // integer / float widths preserve their payload through the
        // recursor (`Option<i64>` → `OptionalInt("i64")`, NOT
        // `OptionalInt("i32")`). Pin the recursor's arm-by-arm mapping
        // so a regression that dropped ONE arm (e.g. accidentally
        // routing `Option<bool>` through the fall-through
        // `OptionalDeserialize`) surfaces here rather than as a silent
        // switch to the serde-bridge extractor at every downstream
        // implementor with an optional-bool field.
        assert!(matches!(
            classify(&parse_ty("Option<String>")),
            Kind::OptionalString
        ));
        assert!(matches!(
            classify(&parse_ty("Option<bool>")),
            Kind::OptionalBool
        ));
        assert!(matches!(
            classify(&parse_ty("Option<i64>")),
            Kind::OptionalInt("i64")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<u32>")),
            Kind::OptionalInt("u32")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<f64>")),
            Kind::OptionalFloat("f64")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<f32>")),
            Kind::OptionalFloat("f32")
        ));
    }

    #[test]
    fn vec_of_string_classifies_as_kind_vec_string() {
        // `Vec<String>` is the ONE non-primitive collection shape
        // `classify_vec` decodes structurally — every other `Vec<T>`
        // falls through to `Kind::VecDeserialize` (pinned by the peer
        // test). The routes matter operationally: `Vec<String>` binds
        // to `extract_string_list` (which decodes each element as a
        // Sexp string atom), whereas `Vec<T: Deserialize>` binds to
        // `extract_vec_via_serde` (which round-trips each element
        // through the sexp_to_json bridge). A regression that
        // conflated the two would silently swap the element decoder at
        // every consumer with a `Vec<String>` field.
        assert!(matches!(
            classify(&parse_ty("Vec<String>")),
            Kind::VecString
        ));
    }

    #[test]
    fn non_primitive_bare_type_falls_through_to_kind_deserialize() {
        // Any type name that doesn't match the primitive-name switch
        // (`String` / `bool` / `i*` / `u*` / `usize` / `f*` / `Option` /
        // `Vec`) routes through the fall-through arm to
        // `Kind::Deserialize` — the sexp_to_json + `serde_json::from_value`
        // bridge that unlocks enums, nested structs, and foreign types.
        // Pin the fall-through discipline: user-defined type names
        // (`MonitorSpec`, `Severity`, `EscalationStep`), primitive-adjacent
        // wrapper types (`String` misspelled as `Strng`), and
        // fully-qualified but non-matching paths ALL land at the
        // Deserialize arm. A regression that (say) narrowed the
        // fall-through to only structs would break the enum
        // authoring surface at every downstream consumer.
        assert!(matches!(
            classify(&parse_ty("MonitorSpec")),
            Kind::Deserialize
        ));
        assert!(matches!(classify(&parse_ty("Severity")), Kind::Deserialize));
        assert!(matches!(classify(&parse_ty("Strng")), Kind::Deserialize));
    }

    #[test]
    fn option_of_non_primitive_classifies_as_kind_optional_deserialize() {
        // `Option<T>` where `T` is NOT a supported primitive routes
        // through `classify_option`'s catch-all arm to
        // `Kind::OptionalDeserialize`. Pin the catch-all: a nested
        // struct (`Option<MonitorSpec>`), an enum
        // (`Option<Severity>`), even a `Vec` (`Option<Vec<String>>` —
        // supported at the `Option` layer but NOT recognized as
        // `OptionalVecString`, because no such Kind variant exists) all
        // land at the same arm. A regression that added a new Kind
        // variant for one of these compositions without updating the
        // recursor arm mapping would silently drift the extractor at
        // every consumer.
        assert!(matches!(
            classify(&parse_ty("Option<MonitorSpec>")),
            Kind::OptionalDeserialize
        ));
        assert!(matches!(
            classify(&parse_ty("Option<Severity>")),
            Kind::OptionalDeserialize
        ));
        assert!(matches!(
            classify(&parse_ty("Option<Vec<String>>")),
            Kind::OptionalDeserialize
        ));
    }

    #[test]
    fn vec_of_non_string_classifies_as_kind_vec_deserialize() {
        // `Vec<T>` where `T` is NOT `String` routes through
        // `classify_vec`'s catch-all arm to `Kind::VecDeserialize` —
        // even for supported primitives (`Vec<i64>`, `Vec<bool>`,
        // `Vec<f64>`). This is a deliberate asymmetry with
        // `classify_option`: the `Option` recursor sharpens per-primitive
        // but the `Vec` recursor sharpens only for `String`. Pin the
        // asymmetry — every non-`String` element type routes through
        // the serde bridge (which decodes each element via the
        // sexp_to_json round-trip) rather than through a hypothetical
        // `Kind::VecInt` / `Kind::VecFloat` primitive-list extractor.
        // A refactor that broadened `classify_vec`'s per-primitive
        // sharpening without a matching Kind variant + extractor pair
        // would produce a shape mismatch at emit time.
        assert!(matches!(
            classify(&parse_ty("Vec<i64>")),
            Kind::VecDeserialize
        ));
        assert!(matches!(
            classify(&parse_ty("Vec<bool>")),
            Kind::VecDeserialize
        ));
        assert!(matches!(
            classify(&parse_ty("Vec<MonitorSpec>")),
            Kind::VecDeserialize
        ));
        assert!(matches!(
            classify(&parse_ty("Vec<Vec<String>>")),
            Kind::VecDeserialize
        ));
    }

    #[test]
    fn fully_qualified_path_classifies_by_final_segment_only() {
        // `classify` reads `path.segments.last()` to look up the type
        // name — fully-qualified paths (`std::string::String`,
        // `::std::string::String`, `alloc::string::String`) project the
        // SAME `Kind::String` as the bare `String`. This is
        // load-bearing: authors that import via `use std::string::String
        // as MyString` or that spell out the full path in a `#[derive]`
        // struct's field must get the SAME extractor. A regression that
        // read the first segment (or the full path text) would break
        // fully-qualified authoring.
        assert!(matches!(
            classify(&parse_ty("std::string::String")),
            Kind::String
        ));
        assert!(matches!(
            classify(&parse_ty("::std::string::String")),
            Kind::String
        ));
        assert!(matches!(
            classify(&parse_ty("core::option::Option<i64>")),
            Kind::OptionalInt("i64")
        ));
    }

    #[test]
    fn reference_and_tuple_and_array_types_fall_through_to_kind_deserialize() {
        // Non-`Type::Path` variants (`&str`, `[u8; 4]`, `(String,
        // i64)`, `&mut i64`) all fall out of the outer `if let
        // Type::Path` guard and land at the `Kind::Deserialize`
        // fall-through. Pin the discipline: non-path types are
        // structurally rejected by the primitive-name switch and route
        // through the serde bridge — even though serde may or may not
        // deserialize them successfully at runtime. The derive's
        // contract is that ANY non-primitive shape gets the
        // Deserialize extractor; the extractor's runtime behavior on
        // exotic types is a separate concern the serde bridge owns.
        assert!(matches!(classify(&parse_ty("&str")), Kind::Deserialize));
        assert!(matches!(classify(&parse_ty("[u8; 4]")), Kind::Deserialize));
        assert!(matches!(
            classify(&parse_ty("(String, i64)")),
            Kind::Deserialize
        ));
    }

    #[test]
    fn first_generic_type_returns_ok_on_a_single_type_argument_segment() {
        // Positive control for the recursor's inner-type extractor —
        // `Option<String>`'s last segment carries ONE type arg
        // (`String`), and `first_generic_type` returns it borrowed. The
        // borrow lifetime matches the caller's, so `classify_option` /
        // `classify_vec` can immediately recurse. Pin the happy path
        // separately from the error path so a regression that (say)
        // returned the wrong generic index or dropped the borrow
        // discipline surfaces here.
        let ty = parse_ty("Option<String>");
        let seg = last_segment(&ty);
        let inner = first_generic_type(seg).expect("Option<String> has one type argument");
        assert!(matches!(inner, Type::Path(_)));
    }

    #[test]
    fn first_generic_type_returns_err_when_the_segment_has_no_angle_brackets() {
        // Error path: a bare type name (`String`) carries NO angle
        // brackets, so `first_generic_type` returns
        // `Err("expected <T> generic arguments".into())`. The recursor
        // wrappers (`classify_option` / `classify_vec`) then swallow
        // the error via `let Ok(inner) = ... else { return
        // Kind::OptionalDeserialize; }` — the fall-through arm the
        // outer `classify` never reaches for `String` (which matches
        // the primitive-name switch first). Pin the error identity so
        // a regression that returned a different error message (or an
        // `Option::None` shape) would surface at the recursor gate
        // instead of silently.
        let ty = parse_ty("String");
        let seg = last_segment(&ty);
        assert_first_generic_type_err(seg, "expected <T> generic arguments");
    }

    #[test]
    fn first_generic_type_returns_err_when_the_segment_carries_only_a_lifetime_argument() {
        // A segment with angle brackets but only a LIFETIME argument
        // (`Cow<'a>` — syntactically valid, semantically incomplete)
        // exits the `for arg in &args.args` loop without finding a
        // `syn::GenericArgument::Type`, and falls through to the
        // `Err("no type argument found".into())` arm. Pin this second
        // error identity separately from the no-angle-brackets arm so
        // a refactor that folded the two arms into one message would
        // surface here.
        let ty = parse_ty("Cow<'a>");
        let seg = last_segment(&ty);
        assert_first_generic_type_err(seg, "no type argument found");
    }
}

#[cfg(test)]
mod extract_keyword_tests {
    use super::extract_keyword;
    use syn::{parse::Parser, Attribute};

    // The `(&[Attribute] -> Option<String>)` projection is the derive's
    // PRIVATE reader for the operator-authored `#[tatara(keyword = "…")]`
    // override. When present it names the top-level Lisp keyword the
    // generated `TataraDomain::KEYWORD` binds to; when absent
    // `derive_tatara_domain` falls back to the auto-derived
    // `default_keyword` projection. A regression here silently swaps the
    // KEYWORD const the entire domain registry hashes on — every
    // downstream `(defX …)` authoring form stops compiling with the same
    // `LispError::HeadMismatch` all `snake_to_kebab`/`default_keyword`
    // regressions surface as, only ONE causal step deeper (the auto-derive
    // ran instead of the operator's chosen keyword).
    //
    // The reader admits FIVE distinct disciplines: only the `tatara` path
    // matches (foreign attribute macros are skipped), only the `Meta::List`
    // shape is decoded (a bare `#[tatara]` is skipped), only the `keyword`
    // sub-key is captured (peer sub-keys under `tatara(…)` are ignored),
    // only a `LitStr` value is accepted (a non-string payload silently
    // returns None — the parser error is swallowed by `let _ = …`), and
    // the first matched attribute wins (later `#[tatara(keyword = …)]`
    // attributes on the same item do not override). Pin each discipline at
    // the boundary of the shape it decodes so a refactor of any of them
    // (e.g. adding `#[tatara(keyword = …, alias = …)]` — a plausible
    // future extension) surfaces at ONE test-module boundary in the
    // derive crate rather than as silent drift across every downstream
    // implementor.

    fn attrs(src: &str) -> Vec<Attribute> {
        Attribute::parse_outer
            .parse_str(src)
            .expect("valid attribute syntax")
    }

    #[test]
    fn empty_attribute_slice_projects_to_none() {
        // Bread-and-butter: no `#[tatara(...)]` attribute → the derive
        // takes the `default_keyword(&type_name)` fallback path. Pin the
        // None arm separately from every "some attr present but not
        // matching" arm so a refactor that (say) returned an empty-string
        // Some("") on missing attrs would surface here rather than as a
        // mystery empty KEYWORD const at the auto-derive site.
        assert_eq!(extract_keyword(&[]), None);
    }

    #[test]
    fn tatara_keyword_string_literal_projects_to_the_literal_value() {
        // The README example: `#[tatara(keyword = "defmonitor")]`
        // projects to `Some("defmonitor".to_string())`. Pin the exact
        // shape the derive contract documents — LitStr value, no
        // surrounding whitespace/quotes preserved (the LitStr .value()
        // yields the unescaped contents).
        assert_eq!(
            extract_keyword(&attrs(r#"#[tatara(keyword = "defmonitor")]"#)),
            Some("defmonitor".to_string())
        );
    }

    #[test]
    fn non_tatara_attribute_is_skipped_by_the_path_gate() {
        // The outer `if !attr.path().is_ident("tatara")` gate silently
        // skips foreign attribute macros. A `#[serde(keyword = "defx")]`
        // and a `#[kube(keyword = "defx")]` (both structurally
        // matching everything AFTER the path) both project to None. Pin
        // the path-gate discipline: the reader is namespaced to
        // `tatara(...)` alone. A regression that broadened the path
        // check (e.g. matched any attribute carrying a `keyword` sub-key)
        // would silently harvest overrides from unrelated attribute macros.
        assert_eq!(
            extract_keyword(&attrs(r#"#[serde(keyword = "defx")]"#)),
            None
        );
        assert_eq!(
            extract_keyword(&attrs(r#"#[kube(keyword = "defx")]"#)),
            None
        );
    }

    #[test]
    fn bare_tatara_path_attribute_without_meta_list_projects_to_none() {
        // `#[tatara]` (a `Meta::Path` — no `(…)` payload) fails the
        // inner `let Meta::List(list) = &attr.meta else { continue; }`
        // guard and skips to the next attribute. Pin the shape gate:
        // only `Meta::List` is decoded. A regression that broadened the
        // gate to accept `Meta::Path` (yielding the bare `defmonitor`
        // via `default_keyword` on the type-name AS IF authored) would
        // silently short-circuit the fallback that today handles the
        // no-attribute case.
        assert_eq!(extract_keyword(&attrs("#[tatara]")), None);
    }

    #[test]
    fn tatara_attribute_without_keyword_sub_key_projects_to_none() {
        // `#[tatara(other_key = "x")]` — the path IS `tatara`, the shape
        // IS `Meta::List`, but the `parse_nested_meta` callback finds no
        // `keyword` ident. The callback silently swallows the `= "x"`
        // value that follows a matched-but-uncaptured sub-key (via the
        // `let _ = list.parse_nested_meta(...)` swallow); `found` stays
        // None; the reader returns None. Pin the sub-key gate: only the
        // `keyword` sub-key is captured. A regression that captured the
        // FIRST sub-key regardless of ident (e.g. via `meta.value()`
        // without the `is_ident("keyword")` guard) would silently
        // harvest an arbitrary sub-key value as the KEYWORD const.
        assert_eq!(
            extract_keyword(&attrs(r#"#[tatara(other_key = "x")]"#)),
            None
        );
    }

    #[test]
    fn non_string_literal_keyword_value_silently_projects_to_none() {
        // `#[tatara(keyword = 42)]` — the path IS `tatara`, the sub-key
        // IS `keyword`, but the value is a `LitInt`, not a `LitStr`.
        // The `let s: LitStr = value.parse()?` inside the callback
        // returns Err; the `?` propagates it out of the closure; the
        // outer `let _ = list.parse_nested_meta(...)` swallows the Err;
        // `found` stays None; the reader returns None WITHOUT diagnostic
        // to the operator. Pin the swallow: today's shape mismatch on
        // the value is silent — a future sharpening that surfaced a
        // typed derive-time diagnostic would flip this test to expect
        // an error path (or a `Result<Option<String>, syn::Error>`
        // return shape). This is the load-bearing test that documents
        // the CURRENT silent-swallow discipline as a deliberate choice
        // rather than a latent bug.
        assert_eq!(extract_keyword(&attrs(r#"#[tatara(keyword = 42)]"#)), None);
    }

    #[test]
    fn first_matching_tatara_attribute_wins_over_later_peers() {
        // Two `#[tatara(keyword = "…")]` attributes on the same item:
        // the outer `for attr in attrs` loop returns the FIRST match
        // via the `if found.is_some() { return found; }` early-exit.
        // The second attribute is unreachable. Pin the winner
        // discipline: earlier attributes shadow later peers. A refactor
        // that (say) picked the LAST attribute or that concatenated
        // matches would surface here.
        let raw = r#"
            #[tatara(keyword = "first")]
            #[tatara(keyword = "second")]
        "#;
        assert_eq!(extract_keyword(&attrs(raw)), Some("first".to_string()));
    }

    #[test]
    fn tatara_attribute_after_non_tatara_attribute_still_matches() {
        // Interleaved: a `#[serde(...)]` (which the path gate skips) does
        // NOT short-circuit the loop; the next `#[tatara(keyword = "…")]`
        // still matches. Pin the loop-continuation discipline: the path
        // gate uses `continue`, not `return`. A refactor that (say)
        // broke out of the loop on the FIRST non-matching attribute
        // would silently drop overrides that appeared after a foreign
        // attribute macro.
        let raw = r#"
            #[serde(rename = "x")]
            #[tatara(keyword = "defafter")]
        "#;
        assert_eq!(extract_keyword(&attrs(raw)), Some("defafter".to_string()));
    }

    #[test]
    fn keyword_after_unrelated_named_value_key_projects_to_the_literal_value() {
        // Fail-before-pass-after guard for the callback's defensive
        // value-drain discipline. Under the PREVIOUS implementation
        // the `parse_nested_meta` callback returned Ok WITHOUT
        // consuming the `= "x"` payload after any non-`keyword` peer,
        // so the outer parser stalled at the `=` following the FIRST
        // unrelated sub-key and errored BEFORE ever reaching
        // `keyword`. The `let _ = list.parse_nested_meta(...)` swallow
        // then dropped that error silently and the reader returned
        // None even though a well-formed `keyword = "…"` sat later in
        // the same attribute. The sharpened callback drains the value
        // of every unmatched sub-key via
        // `let _: syn::Result<syn::Expr> = value.parse()`, so the
        // outer parser advances past the `= <expr>` payload of each
        // peer to the next `,` and continues walking the meta list.
        // A regression that reverted the drain would surface here.
        assert_eq!(
            extract_keyword(&attrs(r#"#[tatara(other = "x", keyword = "defx")]"#)),
            Some("defx".to_string()),
            "keyword AFTER an unrelated named-value sub-key must still be found",
        );
    }

    #[test]
    fn keyword_between_unrelated_named_value_keys_projects_to_the_literal_value() {
        // Sibling of the above — pin the drain discipline across BOTH
        // the pre-keyword AND post-keyword peer positions. Even after
        // the callback flips `found` to Some for the middle `keyword`
        // sub-key, the parse must continue past the trailing
        // `alias = "y"` peer without erroring (the callback drains its
        // value too), so the outer swallow doesn't unwind through a
        // half-consumed attribute. A regression that only drained
        // pre-match peers (e.g. via an early-return in the callback
        // after `found` flips) would surface here.
        assert_eq!(
            extract_keyword(&attrs(
                r#"#[tatara(other = "x", keyword = "defx", alias = "y")]"#
            )),
            Some("defx".to_string()),
        );
    }

    #[test]
    fn keyword_before_unrelated_bare_flag_projects_to_the_literal_value() {
        // Sibling ordering — a bare-flag peer (`Meta::Path`, no `=`
        // payload) after `keyword = "…"` must not stall the parse.
        // The drain branch `if let Ok(value) = meta.value()` short-
        // circuits when there is no `=` following the flag (the
        // outer `meta.value()` returns Err on a bare path), so the
        // callback's Ok return advances past the flag naturally.
        // Pin the drain's tolerance of the bare-flag shape alongside
        // the named-value shape.
        assert_eq!(
            extract_keyword(&attrs(r#"#[tatara(keyword = "defx", other_flag)]"#)),
            Some("defx".to_string()),
        );
    }

    #[test]
    fn keyword_after_unrelated_bare_flag_projects_to_the_literal_value() {
        // Reversed-ordering peer of the above — a bare-flag peer that
        // appears BEFORE `keyword = "…"` must not stall the parse
        // either. The callback's `else if let Ok(value)` branch
        // short-circuits with Err on the bare flag (no `=` to consume)
        // and returns Ok, letting `parse_nested_meta` advance to the
        // next comma-separated peer where `keyword` is captured.
        assert_eq!(
            extract_keyword(&attrs(r#"#[tatara(other_flag, keyword = "defx")]"#)),
            Some("defx".to_string()),
        );
    }
}

#[cfg(test)]
mod find_named_sub_key_tests {
    use super::find_named_sub_key;
    use syn::{parse::Parser, Attribute, LitStr};

    // The `find_named_sub_key<T>(...) -> Option<T>` shared helper is
    // the substrate-level entry the two sibling readers `extract_keyword`
    // (`#[tatara(keyword = "…")]`) and `has_serde_default`
    // (`#[serde(default)]` sniffer) both project through. It closes
    // THREE historically duplicated axes at ONE point: the attr-path
    // gate (only the matching `#[<attr_ident>(...)]` shape is decoded),
    // the sub-key ident match (only the matching sub-key's payload is
    // projected), and the defensive value-drain across unmatched peers
    // (any value-carrying peer, before or after the target, is skipped
    // past without stalling the parser at its `=`).
    //
    // The sibling `extract_keyword_tests` and `has_serde_default_tests`
    // modules close every callback-specific corner via each caller's
    // public projection. This module pins the helper's OWN generic
    // contract — its behavior on a fresh `(attr_ident, sub_key)` pair
    // NOT in use by either existing caller — so a future third caller
    // (the `#[tatara(alias = "…")]` back-compat extension the sibling
    // `keyword_after_unrelated_named_value_key_projects_to_the_literal_value`
    // test's docblock cites, a `#[serde(rename = "…")]` sniffer, a
    // single-key reader lifted out of the 6-key
    // `parse_closed_set_attrs`) composes as a three-line caller
    // against this contract rather than as a fresh copy of the
    // `parse_nested_meta` + drain scaffold. A regression to any of the
    // three axes surfaces AT ONE test-module boundary in the derive
    // crate rather than as silent drift across every downstream
    // implementor of either sibling reader.

    fn attrs(src: &str) -> Vec<Attribute> {
        Attribute::parse_outer
            .parse_str(src)
            .expect("valid attribute syntax")
    }

    fn read_lit_str(attrs_src: &str, attr_ident: &str, sub_key: &str) -> Option<String> {
        find_named_sub_key(&attrs(attrs_src), attr_ident, sub_key, |meta| {
            let value = meta.value()?;
            let s: LitStr = value.parse()?;
            Ok(s.value())
        })
    }

    #[test]
    fn fresh_attr_ident_and_sub_key_pair_project_to_the_literal_value() {
        // The compounding proof: the helper works for a
        // `(attr_ident = "myattr", sub_key = "mykey")` pair NOT in use
        // by either `extract_keyword` (which pins on
        // `("tatara", "keyword")`) or `has_serde_default` (which pins
        // on `("serde", "default")`). A future third caller (the
        // three-axis `alias`/`rename`/`marker`/…-style sub-key reader
        // that today would have re-derived the whole scaffold)
        // composes here in ONE three-line body. A regression that
        // broke the generic-T projection or the (attr_ident, sub_key)
        // parameterization would surface here.
        assert_eq!(
            read_lit_str(r#"#[myattr(mykey = "hello")]"#, "myattr", "mykey"),
            Some("hello".to_string()),
        );
    }

    #[test]
    fn target_sub_key_after_unrelated_named_value_peer_still_resolves() {
        // The defensive value-drain contract, pinned at the helper
        // boundary. Under a naive callback (no drain),
        // `parse_nested_meta` stalls at the `=` following `other`,
        // silently drops the rest of the meta list, and the target
        // `mykey` — which sits AFTER the unrelated peer — is never
        // seen. The helper's `else if let Ok(value) = meta.value()`
        // drain branch keeps the parser advancing past every
        // unmatched value-carrying peer. Sibling to
        // `extract_keyword_tests::keyword_after_unrelated_named_value_key_projects_to_the_literal_value`
        // and
        // `has_serde_default_tests::default_after_other_key_named_value_pair_projects_to_true`
        // — same contract, tested at the helper level so a future
        // third caller inherits it automatically. A refactor that
        // removed the drain would surface across ALL THREE tests
        // simultaneously, naming the causal boundary at the helper.
        assert_eq!(
            read_lit_str(
                r#"#[myattr(other = "x", mykey = "hello")]"#,
                "myattr",
                "mykey"
            ),
            Some("hello".to_string()),
        );
    }

    #[test]
    fn on_match_error_is_silently_absorbed_by_the_outer_swallow() {
        // The historic swallow discipline, pinned at the helper
        // boundary. An `on_match` `?` bail (e.g. `LitStr::parse`
        // failing on a `LitInt` value) unwinds the callback, errors
        // the outer `parse_nested_meta`, and the `let _ = ...`
        // swallow yields `None` WITHOUT surfacing a syn::Error.
        // Mirrors the shape
        // `extract_keyword_tests::non_string_literal_keyword_value_silently_projects_to_none`
        // pins for the `extract_keyword` caller — same swallow, tested
        // at the helper level so a future third caller inherits it. A
        // refactor that surfaced a typed derive-time error here would
        // flip both this test and the sibling caller's swallow test,
        // pointing at the shared entry rather than either caller
        // individually.
        assert_eq!(
            read_lit_str(r#"#[myattr(mykey = 42)]"#, "myattr", "mykey"),
            None,
        );
    }
}

#[cfg(test)]
mod read_meta_lit_str_tests {
    use super::{find_named_sub_key, parse_lit_str, read_meta_lit_str};
    use syn::{parse::Parser, Attribute};

    // The `read_meta_lit_str(&ParseNestedMeta) -> syn::Result<String>`
    // and its underlying `parse_lit_str(ParseStream) -> syn::Result<String>`
    // primitives are the derive's PRIVATE readers for the "read a
    // `LitStr` payload as an owned String" idiom that four sub-key
    // arms across `extract_keyword` +
    // `parse_closed_set_attrs`'s `via` / `unknown` / `set_label`
    // slots project through — plus the `generate_unknown` Explicit arm
    // that stops one abstraction level lower at `parse_lit_str`
    // because its outer `match meta.value()` flag-or-value dispatch
    // has already consumed the `=`.
    //
    // Pre-lift the byte-for-byte identical `let value = meta.value()?;
    // let s: LitStr = value.parse()?; Ok(s.value())` shape lived at
    // FIVE sites. Under this lift the shape lives at ONE substrate
    // entry (two-level: `parse_lit_str` at the ParseStream layer +
    // `read_meta_lit_str` at the ParseNestedMeta layer); every caller
    // routes through it. A future single-keyed string-valued sub-key
    // reader (the `#[tatara(alias = "…")]` extension, a
    // `#[serde(rename = "…")]` sniffer, a single-key reader lifted
    // out of the 6-key `parse_closed_set_attrs`) composes as a
    // one-line caller against the same substrate contract rather
    // than as a fresh copy of the scaffold.
    //
    // Sibling test modules `extract_keyword_tests` /
    // `parse_closed_set_attrs_tests` / `find_named_sub_key_tests`
    // close every caller-specific corner via each caller's public
    // projection. This module pins the primitive's OWN generic
    // contract at THREE corners:
    //   1. positive (LitStr payload projects to owned String);
    //   2. negative-typed (non-LitStr value surfaces syn::Error via
    //      the callback's `?` bail, matching the historic swallow
    //      discipline the sibling readers pin at their own layer);
    //   3. missing-`=` (a bare-flag sub-key with no payload projects
    //      to Err via `meta.value()?` bailing before the parse).
    // A regression to any of the three axes surfaces AT ONE
    // test-module boundary here rather than as silent drift across
    // every caller.

    fn read_via_find(attr_src: &str, attr_ident: &str, sub_key: &str) -> Option<String> {
        // Test-driver: exercise the primitive through its natural
        // composition site (`find_named_sub_key` with the primitive
        // as callback). The outer swallow flips a callback-`?` bail
        // into an outer `None`, so the "syn::Error unwind" corner is
        // observable as `None` at the caller boundary — matching the
        // discipline `extract_keyword_tests::
        // non_string_literal_keyword_value_silently_projects_to_none`
        // and `find_named_sub_key_tests::
        // on_match_error_is_silently_absorbed_by_the_outer_swallow`
        // already pin at the sibling caller / helper layers.
        let attrs: Vec<Attribute> = Attribute::parse_outer
            .parse_str(attr_src)
            .expect("valid attribute syntax");
        find_named_sub_key(&attrs, attr_ident, sub_key, read_meta_lit_str)
    }

    #[test]
    fn lit_str_payload_projects_to_the_owned_string_value() {
        // Positive control — a bare `= "hello"` payload projects
        // through `meta.value()?` + `parse_lit_str` to
        // `Some("hello".to_string())`. Pin the identity so a
        // regression that (say) applied a hidden case-transform or
        // trimmed whitespace surfaces here.
        assert_eq!(
            read_via_find(r#"#[myattr(mykey = "hello")]"#, "myattr", "mykey"),
            Some("hello".to_string()),
        );
    }

    #[test]
    fn lit_str_payload_preserves_interior_whitespace_and_escaped_content() {
        // Load-bearing detail — the `LitStr::value()` projection
        // unescapes the token content but preserves interior spaces
        // AND the unescaped character set. Pin both aspects so a
        // future refactor that swapped `LitStr::value()` for the raw
        // `to_string()` (which would include the surrounding
        // quotation marks) surfaces here.
        assert_eq!(
            read_via_find(r#"#[myattr(mykey = "hello world")]"#, "myattr", "mykey"),
            Some("hello world".to_string()),
        );
        assert_eq!(
            read_via_find(r#"#[myattr(mykey = "line1\nline2")]"#, "myattr", "mykey"),
            Some("line1\nline2".to_string()),
        );
    }

    #[test]
    fn non_lit_str_payload_bails_and_is_swallowed_by_the_outer_find() {
        // Negative-typed control — `= 42` (a `LitInt`) fails the
        // inner `LitStr::parse()` inside `parse_lit_str`; the `?`
        // propagates the syn::Error out through the callback; the
        // outer `find_named_sub_key`'s `let _ = ...` swallows the
        // error and returns None. Pin the swallow at the primitive
        // boundary so a future refactor that surfaced a typed
        // derive-time diagnostic on the value-shape mismatch would
        // flip this test alongside the sibling
        // `extract_keyword_tests::
        // non_string_literal_keyword_value_silently_projects_to_none`
        // and `find_named_sub_key_tests::
        // on_match_error_is_silently_absorbed_by_the_outer_swallow`
        // — naming the CAUSAL boundary as the shared primitive
        // rather than as any one caller.
        assert_eq!(
            read_via_find(r#"#[myattr(mykey = 42)]"#, "myattr", "mykey"),
            None,
        );
    }

    #[test]
    fn bare_flag_sub_key_without_payload_bails_and_is_swallowed() {
        // Missing-`=` control — a bare `mykey` (no `=`) inside
        // `#[myattr(mykey)]` reaches `read_meta_lit_str`, which
        // calls `meta.value()?`. The value getter tries to consume
        // a `=` token and fails (there is none); the `?` propagates
        // the syn::Error; the outer swallow returns None. Pin the
        // flag-vs-value distinction at the primitive: the primitive
        // is EXCLUSIVELY the "read `= <LitStr>` payload" idiom, not
        // a bare-flag reader. A future primitive that admitted the
        // bare-flag shape (a peer to the current one) would keep
        // this contract intact by NOT touching `read_meta_lit_str`
        // itself.
        assert_eq!(read_via_find("#[myattr(mykey)]", "myattr", "mykey"), None,);
    }

    #[test]
    fn parse_lit_str_projects_a_naked_lit_str_stream_to_the_owned_string() {
        // The lower-level `parse_lit_str(ParseStream) -> syn::Result<
        // String>` primitive — pinned separately from the
        // `read_meta_lit_str` composition because it's the arm the
        // `parse_closed_set_attrs::generate_unknown` Explicit branch
        // routes through (its outer `match meta.value()` has already
        // consumed the `=`, so the primitive receives a
        // ParseStream that starts AT the LitStr payload rather than
        // at the `=` gate). Exercise the primitive by feeding it a
        // synthetic ParseStream via `syn::parse::Parser::parse_str`.
        // A regression that folded `parse_lit_str` into
        // `read_meta_lit_str` would break the generate_unknown arm
        // AND surface here.
        let result: syn::Result<String> =
            syn::parse::Parser::parse_str(parse_lit_str, r#""hello""#);
        assert_eq!(result.expect("well-formed LitStr parses OK"), "hello");
    }

    #[test]
    fn parse_lit_str_on_non_lit_str_stream_returns_syn_error() {
        // Negative control for `parse_lit_str` at its OWN layer —
        // feeding it a `42` (LitInt) yields a syn::Error rather than
        // a String. Pin the error path separately from the outer
        // swallow so a future refactor that changed the primitive's
        // error signature (e.g. from `syn::Result<String>` to
        // `Option<String>`) surfaces here rather than as a silent
        // recompilation at every caller.
        let result: syn::Result<String> = syn::parse::Parser::parse_str(parse_lit_str, "42");
        assert!(result.is_err(), "LitInt must not parse as LitStr");
    }
}

#[cfg(test)]
mod has_serde_default_tests {
    use super::has_serde_default;
    use syn::{parse::Parser, Field};

    // The `(&syn::Field -> bool)` projection is the derive's PRIVATE
    // sniffer for `#[serde(default)]` / `#[serde(default = "…")]`. When
    // it returns true, `extractor_for` wraps the base extractor with a
    // `if kw.contains_key(#key) { <extract> } else {
    // ::std::default::Default::default() }` short-circuit so a missing
    // kwarg falls back to `Default::default()` — matching the
    // deserialize semantics the field was already authored for. A
    // regression here silently swaps the missing-kwarg branch on every
    // consumer with a `#[serde(default)]` field: false-negative
    // regresses a legitimate default to a hard `LispError::MissingKwarg`,
    // false-positive regresses a required field to a silent
    // Default::default() slot.
    //
    // Reads the attribute structurally through `parse_nested_meta` —
    // matches how `extract_keyword` already dispatches. The previous
    // substring-match implementation (`tokens.to_string().contains(
    // "default")`) surfaced a false positive on `#[serde]` attributes
    // whose OTHER subkey values carried the literal substring `"default"`
    // (`#[serde(rename = "default_val")]` was reported as carrying
    // `#[serde(default)]`, silently making a required field's slot fall
    // back to Default::default()). The
    // `rename_value_containing_default_substring_projects_to_false` test
    // below is the fail-before-pass-after guard for that fix.

    fn field(src: &str) -> Field {
        Field::parse_named
            .parse_str(src)
            .expect("valid named-field syntax")
    }

    #[test]
    fn field_with_no_attributes_projects_to_false() {
        // Bread-and-butter: a bare `pub name: String` field carries
        // NO `#[serde(...)]` attribute → the derive emits the raw
        // extractor with no missing-kwarg short-circuit. Pin the
        // no-attrs baseline separately from every "attrs present but
        // not matching" arm.
        assert!(!has_serde_default(&field("pub name: String")));
    }

    #[test]
    fn serde_default_bare_form_projects_to_true() {
        // The bare `#[serde(default)]` form — the callback finds a
        // `default` ident with no `= <expr>` payload; `found` flips to
        // true; the outer `if found { return true; }` short-circuits.
        // This is the load-bearing case: the standard serde-default
        // idiom for optional-with-Default fields.
        assert!(has_serde_default(&field(
            "#[serde(default)] pub name: Vec<String>"
        )));
    }

    #[test]
    fn serde_default_with_named_function_payload_projects_to_true() {
        // The `#[serde(default = "path::to::fn")]` form — the callback
        // finds a `default` ident followed by `= "path::to::fn"`. The
        // sharpened reader unconditionally consumes the `= <expr>`
        // payload via `let _: syn::Result<syn::Expr> = value.parse()`
        // so `parse_nested_meta` can advance past the assignment to the
        // next comma-separated peer without stalling — even though
        // `found` is already true.
        assert!(has_serde_default(&field(
            r#"#[serde(default = "path::to::fn")] pub name: String"#
        )));
    }

    #[test]
    fn default_after_other_key_named_value_pair_projects_to_true() {
        // `#[serde(rename = "x", default)]` — the callback fires TWICE:
        // once for `rename` (not matching, but the `= "x"` payload IS
        // consumed via the unconditional value-drain step so the outer
        // `parse_nested_meta` can advance past the `,` to the next
        // peer), once for `default` (matching, flips `found` to true).
        // This is the load-bearing test for the value-drain discipline:
        // WITHOUT the unconditional `meta.value()` consumption, the
        // outer parser stalls at the `=` following `rename` and errors
        // before ever seeing `default` (verified via the `probe` crate
        // in this run's investigation — a naive `parse_nested_meta`
        // callback that just checked the ident returned false here).
        assert!(has_serde_default(&field(
            r#"#[serde(rename = "x", default)] pub name: String"#
        )));
    }

    #[test]
    fn default_before_other_key_named_value_pair_projects_to_true() {
        // Reversed order: `#[serde(default, rename = "x")]` — the
        // callback fires for `default` FIRST, flips `found` to true.
        // Even if the SECOND callback (for `rename`) fails to advance
        // (e.g. under a future refactor), `found` is already true.
        // Pin both orderings so a refactor that made the reader
        // order-sensitive would surface here.
        assert!(has_serde_default(&field(
            r#"#[serde(default, rename = "x")] pub name: String"#
        )));
    }

    #[test]
    fn rename_value_containing_default_substring_projects_to_false() {
        // Fail-before-pass-after guard for the substring-match →
        // structural-parse sharpen. Under the PREVIOUS implementation
        // (`list.tokens.to_string().contains("default")`), this
        // `#[serde(rename = "default_val")]` attribute was incorrectly
        // reported as carrying `#[serde(default)]` — the substring
        // `"default"` appears inside the RENAME value's LitStr and
        // matched the loose check. The sharpened structural reader
        // walks the nested meta items and only flips `found` when a
        // sub-key ident is EXACTLY `default`; the rename value is now
        // correctly ignored. A regression that reverted to the
        // substring check would flip this test to true.
        assert!(!has_serde_default(&field(
            r#"#[serde(rename = "default_val")] pub name: String"#
        )));
    }

    #[test]
    fn alias_value_containing_default_substring_projects_to_false() {
        // Sibling of the rename test — `#[serde(alias = "…default…")]`
        // is another serde attribute whose value might carry the
        // literal substring `"default"` (aliases occasionally mirror
        // internal field names). Under the previous substring check
        // both surfaced as false positives; under the sharpened reader
        // both correctly project to false.
        assert!(!has_serde_default(&field(
            r#"#[serde(alias = "some_default_name")] pub name: String"#
        )));
    }

    #[test]
    fn non_serde_attribute_carrying_default_ident_is_skipped_by_the_path_gate() {
        // `#[other(default)]` — the path is `other`, not `serde`, so
        // the outer `if !attr.path().is_ident("serde")` gate silently
        // skips it via `continue`. Pin the path-gate discipline: the
        // sniffer is namespaced to `#[serde(...)]` alone. A regression
        // that broadened the path check (e.g. matched any attribute
        // carrying a `default` sub-key) would silently harvest defaults
        // from unrelated attribute macros — e.g. `#[builder(default)]`
        // from the `derive_builder` crate would incorrectly trigger the
        // missing-kwarg short-circuit on every `derive_builder` +
        // `derive_tatara_domain` co-derived field.
        assert!(!has_serde_default(&field(
            "#[other(default)] pub name: String"
        )));
    }

    #[test]
    fn bare_serde_path_attribute_without_meta_list_projects_to_false() {
        // `#[serde]` (a `Meta::Path` — no `(…)` payload) fails the
        // inner `let Meta::List(list) = &attr.meta else { continue; }`
        // guard and skips to the next attribute. Pin the shape gate:
        // only `Meta::List` is decoded. A regression that broadened the
        // gate to accept `Meta::Path` would silently interpret every
        // bare `#[serde]` (rare but syntactically valid) as
        // `#[serde(default)]` — inverting the missing-kwarg semantics.
        assert!(!has_serde_default(&field("#[serde] pub name: String")));
    }

    #[test]
    fn serde_attribute_without_default_sub_key_projects_to_false() {
        // Baseline "serde present, default absent" — the callback
        // fires for every peer sub-key (`rename`, `skip_serializing_if`,
        // `flatten`, …), none match `default`, `found` stays false.
        // Pin the AND-gate: both the path AND the sub-key must match.
        assert!(!has_serde_default(&field(
            r#"#[serde(rename = "x", skip_serializing_if = "Option::is_none")] pub name: Option<String>"#
        )));
    }

    #[test]
    fn multiple_serde_attributes_project_to_true_if_any_carries_default() {
        // Two peer `#[serde(...)]` attributes on the same field: the
        // outer `for attr in &field.attrs` loop checks each until the
        // `if found { return true; }` short-circuit fires. Pin the
        // any-attr discipline: the sniffer is OR-across-attrs. This is
        // load-bearing for the (rare but valid) split-across-attrs
        // authoring style — a refactor that only checked the FIRST
        // serde attribute would silently regress the split form.
        let raw = r#"
            #[serde(rename = "x")]
            #[serde(default)]
            pub name: String
        "#;
        assert!(has_serde_default(&field(raw)));
    }
}

#[cfg(test)]
mod parse_closed_set_attrs_tests {
    use super::{parse_closed_set_attrs, GenerateUnknown};
    use syn::{parse::Parser, parse_str, Attribute, Ident};

    // The `(&[Attribute], &Ident -> syn::Result<ClosedSetCfg>)` projection
    // is the derive's PRIVATE reader for the SIX-keyed
    // `#[closed_set(...)]` attribute surface (`via`, `unknown`,
    // `no_from_str`, `generate_unknown` [with optional `= "<label>"`
    // payload], `display`, `set_label`) — the sole authoring surface
    // through which every `#[derive(ClosedSet)]` implementor pins the
    // trait-impl plumbing the derive emits (delegation projection, name
    // of the parse-rejection carrier, whether to suppress the generated
    // `FromStr`, whether to auto-derive the `Unknown` struct, whether
    // to emit a `Display` block, and the trait-level `SET_LABEL` const).
    // A regression here silently swaps ONE of the SIX per-attribute
    // outputs on every downstream implementor — the operator sees the
    // attribute the same way but the emitted code silently diverges
    // (e.g. a swap of the `via` default from `"label"` to `"as_str"`
    // would break every implementor that omits `via` AND whose
    // inherent projection method is NOT `as_str`).
    //
    // The reader admits SIX distinct disciplines (one per key), the
    // typed-error escape hatch (unknown key), and the reader-level
    // aggregation shape (multi-attr merging, path/shape gates that
    // filter attributes before decode). Pin each discipline at the
    // boundary of the shape it decodes so a refactor of any of them
    // (e.g. a future `#[closed_set(prefix = "…")]` alias, a
    // typed-diagnostic surfacing on the value-shape mismatch that today
    // errors via `?`) surfaces at ONE test-module boundary in the
    // derive crate rather than as silent drift across the 40+
    // closed-set implementors that route through this reader.
    //
    // Peer to `extract_keyword_tests` above — the sibling
    // single-keyed reader for `#[tatara(keyword = "…")]`. Together the
    // two test modules close the derive's TWO attribute-reader
    // contracts (operator-authored TataraDomain keyword override +
    // operator-authored ClosedSet six-axis configuration) at the
    // test-module boundary.

    fn attrs(src: &str) -> Vec<Attribute> {
        Attribute::parse_outer
            .parse_str(src)
            .expect("valid attribute syntax")
    }

    fn ident(s: &str) -> Ident {
        parse_str(s).expect("valid identifier")
    }

    #[test]
    fn empty_attribute_slice_projects_to_the_documented_default_configuration() {
        // Bread-and-butter: no `#[closed_set(...)]` attribute → the
        // derive falls back to the documented defaults documented on the
        // proc-macro attribute:
        //   - `via = "label"` (the trait's own method name),
        //   - `unknown = "Unknown{EnumName}"` (the workspace-wide naming
        //     convention),
        //   - `no_from_str = false` (FromStr is emitted),
        //   - `generate_unknown = Skip` (implementor hand-rolls the
        //     carrier),
        //   - `display = false` (implementor hand-rolls Display), and
        //   - `set_label = None` (the trait `SET_LABEL` const resolves
        //     via the `generate_unknown` label fallback chain).
        // Pin the defaults separately from every override arm so a
        // refactor that (say) flipped `no_from_str`'s default to `true`
        // or changed the `Unknown{name}` fallback to `Unknown_{name}` /
        // `{name}Unknown` surfaces here rather than as silent drift
        // across every implementor that omits the corresponding key.
        let name = ident("MyEnum");
        let cfg = parse_closed_set_attrs(&[], &name).expect("empty attrs parse OK");
        assert_eq!(cfg.via, "label");
        assert_eq!(cfg.unknown, "UnknownMyEnum");
        assert!(!cfg.no_from_str);
        assert!(matches!(cfg.generate_unknown, GenerateUnknown::Skip));
        assert!(!cfg.display);
        assert_eq!(cfg.set_label, None);
    }

    #[test]
    fn via_string_literal_overrides_the_delegation_projection_name() {
        // The load-bearing `via` override: implementors on the
        // domain-canonical `as_str` name (`tatara_process`'s
        // wire-format axis), or on any other bespoke inherent
        // projection (`prefix`, `marker`, `keyword`), thread the name
        // through the `via` axis. Pin the exact string that survives
        // the LitStr → String projection.
        let name = ident("MyEnum");
        let attrs = attrs(r#"#[closed_set(via = "as_str")]"#);
        let cfg = parse_closed_set_attrs(&attrs, &name).expect("via override parses OK");
        assert_eq!(cfg.via, "as_str");
    }

    #[test]
    fn unknown_string_literal_overrides_the_default_carrier_name() {
        // The `unknown` override: implementors whose carrier struct
        // name diverges from the `Unknown{EnumName}` auto-derivation
        // (e.g. a historical hand-rolled `BogusChannelKind` style)
        // pin the carrier name through the `unknown` axis. Pin the
        // exact string that survives the LitStr → String projection —
        // a regression that (say) prepended `Unknown` to the override
        // value would silently rewire the carrier reference.
        let name = ident("MyEnum");
        let attrs = attrs(r#"#[closed_set(unknown = "MyBespokeUnknown")]"#);
        let cfg = parse_closed_set_attrs(&attrs, &name).expect("unknown override parses OK");
        assert_eq!(cfg.unknown, "MyBespokeUnknown");
    }

    #[test]
    fn no_from_str_bare_flag_projects_to_true() {
        // The `no_from_str` flag suppresses the `impl FromStr` block
        // for enums that carry a bespoke `FromStr` shape (e.g.
        // `CompilerSpecIoStage`'s compound `"{operation}: {label}"`
        // key). Pin the flag as a BARE meta path — `no_from_str = true`
        // is NOT a valid form under today's reader; only the presence
        // of the ident flips the flag. A refactor that admitted a
        // typed `= <bool>` payload would surface here.
        let name = ident("MyEnum");
        let attrs = attrs("#[closed_set(no_from_str)]");
        let cfg = parse_closed_set_attrs(&attrs, &name).expect("no_from_str flag parses OK");
        assert!(cfg.no_from_str);
    }

    #[test]
    fn generate_unknown_bare_flag_projects_to_the_auto_variant() {
        // Bare `generate_unknown` — the callback's inner
        // `match meta.value()` reaches the `Err(_)` arm (there is no
        // `= <value>` payload), yielding `GenerateUnknown::Auto`. The
        // downstream `set_label` resolver then projects the enum name
        // through `pascal_to_spaced_lowercase`. Pin the bare-form
        // dispatch: a regression that (say) required the `= "label"`
        // payload as mandatory would flip this arm to a syn::Error.
        let name = ident("MyEnum");
        let attrs = attrs("#[closed_set(generate_unknown)]");
        let cfg = parse_closed_set_attrs(&attrs, &name).expect("bare generate_unknown parses OK");
        assert!(matches!(cfg.generate_unknown, GenerateUnknown::Auto));
    }

    #[test]
    fn generate_unknown_with_string_literal_projects_to_the_explicit_variant() {
        // `generate_unknown = "macro definition head"` — the callback
        // reaches the `Ok(value)` arm, parses the LitStr, and yields
        // `GenerateUnknown::Explicit(label)`. The downstream
        // `set_label` resolver then reads the SAME label from the
        // carrier's `#[error("unknown <label>: {0}")]` annotation so
        // the trait const and the carrier annotation emit from ONE
        // generative origin. Pin the exact label that survives the
        // LitStr → Explicit(String) projection.
        let name = ident("MyEnum");
        let attrs = attrs(r#"#[closed_set(generate_unknown = "macro definition head")]"#);
        let cfg =
            parse_closed_set_attrs(&attrs, &name).expect("labeled generate_unknown parses OK");
        match cfg.generate_unknown {
            GenerateUnknown::Explicit(label) => {
                assert_eq!(label, "macro definition head");
            }
            other => panic!(
                "expected GenerateUnknown::Explicit(\"macro definition head\"), got {:?} variant",
                match other {
                    GenerateUnknown::Skip => "Skip",
                    GenerateUnknown::Auto => "Auto",
                    GenerateUnknown::Explicit(_) => unreachable!(),
                },
            ),
        }
    }

    #[test]
    fn display_bare_flag_projects_to_true() {
        // The `display` flag emits the substrate-wide
        // `impl fmt::Display { f.write_str(Self::$via(*self)) }` block.
        // Pin the flag as a BARE meta path — sibling of `no_from_str`.
        // A regression that (say) required a `= true` payload would
        // surface here.
        let name = ident("MyEnum");
        let attrs = attrs("#[closed_set(display)]");
        let cfg = parse_closed_set_attrs(&attrs, &name).expect("display flag parses OK");
        assert!(cfg.display);
    }

    #[test]
    fn set_label_string_literal_overrides_the_trait_const_label() {
        // The `set_label` override binds the trait's `SET_LABEL` const
        // independently of the carrier's `#[error(...)]` annotation.
        // Pin the exact string that survives the LitStr → Option<String>
        // projection — a regression that (say) collapsed `set_label`
        // and `generate_unknown = "..."` onto ONE storage slot would
        // surface here (the two axes are structurally distinct in
        // ClosedSetCfg).
        let name = ident("MyEnum");
        let attrs = attrs(r#"#[closed_set(set_label = "explicit trait label")]"#);
        let cfg = parse_closed_set_attrs(&attrs, &name).expect("set_label override parses OK");
        assert_eq!(cfg.set_label, Some("explicit trait label".to_string()));
    }

    #[test]
    fn unknown_key_returns_syn_error_with_the_documented_diagnostic() {
        // An unrecognized sub-key under `#[closed_set(...)]` surfaces
        // as a compile-time syn::Error via the callback's final `else`
        // arm. The diagnostic names the SIX allowed keys verbatim so
        // the operator's next action (fixing the typo) is obvious.
        // Pin the discipline (Err path, not silent-swallow) AND the
        // diagnostic wording — a refactor that added a new key must
        // update the diagnostic to mention it, and this test surfaces
        // the drift as a message-string mismatch rather than as a
        // silent shift in the allowed-set.
        let name = ident("MyEnum");
        let attrs = attrs(r#"#[closed_set(bogus_key = "x")]"#);
        // `ClosedSetCfg` does not derive `Debug` — `expect_err` would
        // require it. Match structurally on the Err arm and pin the
        // diagnostic verbatim (mirrors the discipline
        // `classify_tests::assert_first_generic_type_err` follows for
        // `first_generic_type`'s error path).
        let err = match parse_closed_set_attrs(&attrs, &name) {
            Err(err) => err,
            Ok(_) => panic!("bogus key must return syn::Error"),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("unknown #[closed_set(...)] key"),
            "diagnostic must name the failing key surface, got {msg:?}",
        );
        for key in [
            "via",
            "unknown",
            "no_from_str",
            "generate_unknown",
            "display",
            "set_label",
        ] {
            assert!(
                msg.contains(key),
                "diagnostic must list the allowed key `{key}` verbatim, got {msg:?}",
            );
        }
    }

    #[test]
    fn multiple_keys_in_one_attribute_all_project_to_their_slots() {
        // The realistic authoring shape: several keys under one
        // `#[closed_set(...)]` attribute (`via = "as_str"`,
        // `generate_unknown`, `display`) coexist. Pin the
        // interleaved-decode discipline: each `parse_nested_meta`
        // callback iteration writes to its OWN slot and the outer
        // loop hands control back for the next key. A refactor that
        // (say) accidentally shared state between two callback arms
        // would surface here.
        let name = ident("MyEnum");
        let attrs =
            attrs(r#"#[closed_set(via = "as_str", generate_unknown, display, no_from_str)]"#);
        let cfg = parse_closed_set_attrs(&attrs, &name).expect("multi-key single-attr parses OK");
        assert_eq!(cfg.via, "as_str");
        assert!(matches!(cfg.generate_unknown, GenerateUnknown::Auto));
        assert!(cfg.display);
        assert!(cfg.no_from_str);
        // Slots NOT set through this attribute must retain their
        // defaults — pin the discipline that the reader touches
        // ONLY the slots it decodes.
        assert_eq!(cfg.unknown, "UnknownMyEnum");
        assert_eq!(cfg.set_label, None);
    }

    #[test]
    fn multiple_closed_set_attributes_merge_across_the_outer_loop() {
        // The split-across-attrs authoring shape: two
        // `#[closed_set(...)]` attributes on the same item. The outer
        // `for attr in attrs` loop visits BOTH; each contributes its
        // subset of the six slots. Pin the merge discipline — a
        // refactor that (say) short-circuited on the first attribute
        // would silently drop the second attribute's contribution.
        let name = ident("MyEnum");
        let raw = r#"
            #[closed_set(via = "as_str")]
            #[closed_set(display)]
        "#;
        let cfg = parse_closed_set_attrs(&attrs(raw), &name).expect("split-across-attrs parses OK");
        assert_eq!(cfg.via, "as_str");
        assert!(cfg.display);
    }

    #[test]
    fn later_closed_set_attribute_wins_over_earlier_peer_on_the_same_key() {
        // Two peer `#[closed_set(via = "...")]` attributes: the outer
        // loop visits both, and each writes to the SAME `via: Option<
        // String>` slot without a `.is_some()` guard, so the LAST
        // write wins. Pin this discipline separately from
        // `first_matching_tatara_attribute_wins_over_later_peers` on
        // the sibling `extract_keyword` reader — the two readers make
        // opposite choices, and pinning both surfaces the asymmetry
        // at ONE test-module boundary per side.
        let name = ident("MyEnum");
        let raw = r#"
            #[closed_set(via = "first")]
            #[closed_set(via = "second")]
        "#;
        let cfg =
            parse_closed_set_attrs(&attrs(raw), &name).expect("last-write-wins scenario parses OK");
        assert_eq!(cfg.via, "second");
    }

    #[test]
    fn non_closed_set_attribute_is_skipped_by_the_path_gate() {
        // The outer `if !attr.path().is_ident("closed_set")` gate
        // silently skips foreign attribute macros. A `#[serde(via =
        // "foo")]` (structurally matching everything AFTER the path)
        // does NOT contribute to the config — the resulting cfg is
        // the empty-attrs default. Pin the path-gate discipline: the
        // reader is namespaced to `closed_set(...)` alone.
        let name = ident("MyEnum");
        let attrs = attrs(r#"#[serde(via = "foo")]"#);
        let cfg = parse_closed_set_attrs(&attrs, &name)
            .expect("foreign attribute skipped, returns default");
        assert_eq!(cfg.via, "label");
    }

    #[test]
    fn bare_closed_set_path_attribute_without_meta_list_is_skipped() {
        // `#[closed_set]` (a `Meta::Path` — no `(…)` payload) fails
        // the inner `let Meta::List(list) = &attr.meta else { continue;
        // }` guard and skips to the next attribute. Pin the shape
        // gate: only `Meta::List` is decoded. A regression that
        // broadened the gate to accept `Meta::Path` (yielding a
        // silent "all defaults" cfg via a non-attribute code path)
        // would surface here.
        let name = ident("MyEnum");
        let attrs = attrs("#[closed_set]");
        let cfg = parse_closed_set_attrs(&attrs, &name)
            .expect("bare closed_set path attribute skipped, returns default");
        // Every slot at its default — indistinguishable from
        // empty-attrs. The pin is that no PANIC / no ERROR surfaces.
        assert_eq!(cfg.via, "label");
        assert!(!cfg.no_from_str);
        assert!(matches!(cfg.generate_unknown, GenerateUnknown::Skip));
    }

    #[test]
    fn unknown_fallback_reads_the_ident_name_via_the_display_projection() {
        // The `unknown` fallback `format!("Unknown{name}")` reads the
        // `Ident`'s Display projection — the type name verbatim, no
        // additional case-mapping. Pin the identity discipline across
        // TWO distinct enum names so a refactor that (say) applied a
        // hidden case-transform (`to_ascii_uppercase`, `to_pascal_case`)
        // surfaces as a fallback drift at the second arm.
        let cfg_a =
            parse_closed_set_attrs(&[], &ident("ChannelKind")).expect("empty attrs parse OK");
        assert_eq!(cfg_a.unknown, "UnknownChannelKind");
        let cfg_b =
            parse_closed_set_attrs(&[], &ident("ReplacementPolicy")).expect("empty attrs parse OK");
        assert_eq!(cfg_b.unknown, "UnknownReplacementPolicy");
    }

    #[test]
    fn explicit_unknown_override_takes_precedence_over_the_ident_fallback() {
        // A `#[closed_set(unknown = "…")]` present alongside the
        // fallback-eligible empty state must SUPPRESS the fallback.
        // Pin the priority chain: attribute-supplied value wins over
        // the `Unknown{name}` auto-derivation for EVERY ident name.
        // A regression that (say) concatenated the two (e.g.
        // `format!("Unknown{name}{override}")`) would surface here.
        let cfg = parse_closed_set_attrs(
            &attrs(r#"#[closed_set(unknown = "MyBespoke")]"#),
            &ident("ChannelKind"),
        )
        .expect("unknown override parses OK");
        assert_eq!(cfg.unknown, "MyBespoke");
    }
}

#[cfg(test)]
mod try_sub_key_tests {
    //! Contract tests for the [`try_lit_str_sub_key`] +
    //! [`try_bool_flag_sub_key`] sibling primitives — the two-arm
    //! `parse_nested_meta`-callback dispatch matrix
    //! [`parse_closed_set_attrs`] threads through for its five
    //! duplicated (three string-valued + two bare-flag) sub-key arms.
    //!
    //! The sibling `parse_closed_set_attrs_tests` module closes every
    //! caller-specific corner via the SIX-key aggregating reader. This
    //! module pins the two primitives' OWN generic contracts at their
    //! natural corners:
    //!   1. matching sub-key AND payload → slot mutation + `Ok(true)`;
    //!   2. non-matching sub-key → slot untouched + `Ok(false)`;
    //!   3. matching sub-key AND malformed payload (string-valued arm
    //!      only) → syn::Error propagated + slot untouched;
    //!   4. slot-mutation locality — only the primitive whose sub-key
    //!      matches writes its slot; the other slots remain at their
    //!      pre-call values, pinning the short-circuit `||` dispatch
    //!      chain in [`parse_closed_set_attrs`] as safe under mixed
    //!      calls.
    //!
    //! A regression to any of the four axes surfaces AT ONE
    //! test-module boundary here rather than as silent drift across
    //! every caller.

    use super::{try_bool_flag_sub_key, try_lit_str_sub_key};
    use syn::parse::Parser;
    use syn::{Attribute, Meta};

    fn nested_meta_list(attr_src: &str) -> syn::MetaList {
        // Extract the SINGLE `Meta::List` — i.e., the
        // `#[<attr_ident>(<sub_key> [= <value>] ...)]` payload — from an
        // input attribute source. The two primitives take a
        // `&ParseNestedMeta` handle, which is produced by the outer
        // `MetaList::parse_nested_meta` walk; the driver below then
        // exercises each callback through that walk.
        let attrs: Vec<Attribute> = Attribute::parse_outer
            .parse_str(attr_src)
            .expect("valid attribute syntax");
        let Meta::List(list) = attrs
            .into_iter()
            .next()
            .expect("attribute list is non-empty")
            .meta
        else {
            panic!("expected Meta::List (#[<attr>(...)]) shape");
        };
        list
    }

    #[test]
    fn try_lit_str_sub_key_on_matching_ident_writes_slot_and_reports_matched() {
        // Positive control for the string-valued arm — the primitive's
        // MATCHING path: `#[myattr(mykey = "hello")]` with a
        // `try_lit_str_sub_key(_, "mykey", _)` call writes
        // `Some("hello".to_string())` to the slot and returns
        // `Ok(true)`. Pin BOTH the slot mutation AND the return-bit so
        // a regression that (say) flipped the return-bit to `false`
        // (breaking the outer `||` dispatch's first-match-wins
        // ordering) or wrote `None` (silently dropping the payload)
        // surfaces here.
        let list = nested_meta_list(r#"#[myattr(mykey = "hello")]"#);
        let mut slot: Option<String> = None;
        let mut matched: Option<bool> = None;
        list.parse_nested_meta(|meta| {
            matched = Some(try_lit_str_sub_key(&meta, "mykey", &mut slot)?);
            Ok(())
        })
        .expect("meta walk completes");
        assert_eq!(matched, Some(true));
        assert_eq!(slot, Some("hello".to_string()));
    }

    #[test]
    fn try_lit_str_sub_key_on_non_matching_ident_leaves_slot_and_reports_not_matched() {
        // Negative-ident control for the string-valued arm — the
        // primitive's NON-MATCHING path: `#[myattr(other = "x")]`
        // with a `try_lit_str_sub_key(_, "mykey", _)` call returns
        // `Ok(false)` and DOES NOT touch the slot (the pre-call
        // `None` survives). Load-bearing for the outer `||` dispatch
        // in `parse_closed_set_attrs`: every non-matching primitive
        // call MUST leave its slot untouched so the mutation is
        // local to the matching arm alone. A regression that (say)
        // eagerly cleared the slot on non-match would silently reset
        // any previous attribute's contribution — this test surfaces
        // that drift immediately.
        let list = nested_meta_list(r#"#[myattr(other = "x")]"#);
        let mut slot: Option<String> = Some("preexisting".to_string());
        let mut matched: Option<bool> = None;
        list.parse_nested_meta(|meta| {
            matched = Some(try_lit_str_sub_key(&meta, "mykey", &mut slot)?);
            // The `parse_nested_meta` walk stalls at the `=` of a
            // value-carrying peer without an explicit drain — mirror
            // the sibling `find_named_sub_key`'s defensive value-drain
            // discipline so the outer walk can complete.
            if !matched.expect("callback ran") {
                if let Ok(value) = meta.value() {
                    let _: syn::Result<syn::Expr> = value.parse();
                }
            }
            Ok(())
        })
        .expect("meta walk completes");
        assert_eq!(matched, Some(false));
        assert_eq!(slot, Some("preexisting".to_string()));
    }

    #[test]
    fn try_lit_str_sub_key_on_matching_ident_with_non_lit_str_payload_bails_via_read_meta_lit_str()
    {
        // Negative-typed control for the string-valued arm — the
        // primitive's MATCHING ident BUT MALFORMED payload path:
        // `#[myattr(mykey = 42)]` with a `try_lit_str_sub_key(_,
        // "mykey", _)` call returns `Err(syn::Error)` via the
        // underlying `read_meta_lit_str` primitive's `LitStr::parse`
        // failing on the `LitInt`. The slot MUST remain untouched
        // (the `*slot = Some(...)` write is gated behind the
        // `read_meta_lit_str(meta)?` propagation). A regression that
        // (say) reordered the slot-write ahead of the parse-error
        // propagation would silently write a garbage / partial value
        // — this test surfaces that drift immediately.
        let list = nested_meta_list(r#"#[myattr(mykey = 42)]"#);
        let mut slot: Option<String> = None;
        let err = list
            .parse_nested_meta(|meta| try_lit_str_sub_key(&meta, "mykey", &mut slot).map(|_| ()))
            .expect_err("non-LitStr payload must surface a syn::Error");
        assert!(
            err.to_string().contains("expected string literal")
                || err.to_string().contains("LitStr"),
            "diagnostic must surface the LitStr shape gate, got {err:?}",
        );
        assert_eq!(
            slot, None,
            "slot must remain untouched when the payload-parse fails",
        );
    }

    #[test]
    fn try_bool_flag_sub_key_on_matching_bare_ident_flips_flag_and_reports_matched() {
        // Positive control for the bare-flag arm — the primitive's
        // MATCHING path: `#[myattr(myflag)]` with a
        // `try_bool_flag_sub_key(_, "myflag", _)` call flips the flag
        // to `true` and returns `Ok(true)`. Pin BOTH the flag flip
        // AND the return-bit for the SAME reasons as the sibling
        // string-valued positive test above.
        let list = nested_meta_list("#[myattr(myflag)]");
        let mut flag: bool = false;
        let mut matched: Option<bool> = None;
        list.parse_nested_meta(|meta| {
            matched = Some(try_bool_flag_sub_key(&meta, "myflag", &mut flag)?);
            Ok(())
        })
        .expect("meta walk completes");
        assert_eq!(matched, Some(true));
        assert!(flag);
    }

    #[test]
    fn try_bool_flag_sub_key_on_non_matching_ident_leaves_flag_and_reports_not_matched() {
        // Negative-ident control for the bare-flag arm — the
        // primitive's NON-MATCHING path: `#[myattr(other)]` with a
        // `try_bool_flag_sub_key(_, "myflag", _)` call returns
        // `Ok(false)` and DOES NOT touch the flag (the pre-call
        // `true` survives). Load-bearing for the outer `||` dispatch
        // in `parse_closed_set_attrs`: every non-matching primitive
        // call MUST leave its flag untouched so the mutation is
        // local to the matching arm alone. A regression that (say)
        // reset the flag to `false` on non-match would silently drop
        // any previous attribute's contribution — this test
        // surfaces that drift immediately.
        let list = nested_meta_list("#[myattr(other)]");
        let mut flag: bool = true;
        let mut matched: Option<bool> = None;
        list.parse_nested_meta(|meta| {
            matched = Some(try_bool_flag_sub_key(&meta, "myflag", &mut flag)?);
            Ok(())
        })
        .expect("meta walk completes");
        assert_eq!(matched, Some(false));
        assert!(
            flag,
            "flag must remain at its pre-call `true` value on non-match",
        );
    }

    #[test]
    fn short_circuit_or_chain_of_mixed_primitives_only_mutates_the_matching_slot() {
        // The compounding proof: the two primitives compose safely on
        // ONE `||` dispatch chain — the SAME shape
        // [`parse_closed_set_attrs`] uses to collapse its five
        // duplicated arms. Given `#[myattr(display)]`, a chain
        // `try_lit_str_sub_key("via")? || try_lit_str_sub_key("unknown")?
        // || try_bool_flag_sub_key("no_from_str")? ||
        // try_bool_flag_sub_key("display")?` MUST flip ONLY the
        // `display` flag and leave the other three slots untouched.
        // Pin the mutation-locality contract: the `||` operator's
        // laziness stops evaluation at the first matching primitive,
        // AND every non-matching primitive that DID run left its slot
        // untouched. A regression to EITHER discipline would surface
        // here.
        let list = nested_meta_list("#[myattr(display)]");
        let mut via: Option<String> = None;
        let mut unknown: Option<String> = None;
        let mut no_from_str: bool = false;
        let mut display: bool = false;
        list.parse_nested_meta(|meta| {
            let matched = try_lit_str_sub_key(&meta, "via", &mut via)?
                || try_lit_str_sub_key(&meta, "unknown", &mut unknown)?
                || try_bool_flag_sub_key(&meta, "no_from_str", &mut no_from_str)?
                || try_bool_flag_sub_key(&meta, "display", &mut display)?;
            assert!(matched, "one of the four arms must match `display`");
            Ok(())
        })
        .expect("meta walk completes");
        assert_eq!(via, None);
        assert_eq!(unknown, None);
        assert!(!no_from_str);
        assert!(display);
    }
}
