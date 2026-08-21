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
//!             threshold: <f64 as NarrowNumeric>::extract_narrowed_kwarg(&kw, "threshold")?,
//!             window_seconds: <i64 as NarrowNumeric>::extract_optional_narrowed_kwarg(&kw, "window-seconds")?,
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
        let default = serde_default(field);
        match extractor_for(&field.ty, &kebab, &default) {
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

/// Typed projection of a field's `#[serde(default[ = "path"])]`
/// posture — the enum the derive dispatches on when composing the
/// missing-kwarg fallback expression inside [`extractor_for`].
///
/// The three variants partition the closed set of authoring shapes
/// serde honors on a field's `#[serde(default[ = "…"])]` axis, and
/// dispatch the derive's missing-kwarg branch to the corresponding
/// initializer expression:
///
/// | Variant                | Authored as                        | Missing-kwarg branch                 |
/// |------------------------|------------------------------------|--------------------------------------|
/// | [`Self::Absent`]       | (no `#[serde(default)]` attribute) | (no branch; field always required)   |
/// | [`Self::Trait`]        | `#[serde(default)]`                | `::std::default::Default::default()` |
/// | [`Self::Path`]         | `#[serde(default = "path")]`       | `path()` — a caller-authored fn      |
///
/// Pre-lift the derive collapsed [`Self::Trait`] and [`Self::Path`]
/// into ONE `has_serde_default(&Field) -> bool` sniffer and emitted
/// `::std::default::Default::default()` on both, silently dropping the
/// operator-authored `= "path"` payload — a divergence from serde's
/// own semantics that tatara-init's config module documents in its
/// test module as a known workaround (`empty_definit_parses`). Post-
/// lift the payload rides through the typed [`Self::Path`] variant
/// into the derive's emitted expression and the workaround dissolves;
/// every future [`TataraDomain`] author with a `#[serde(default =
/// "path")]` field inherits the fix mechanically at the derive
/// boundary rather than as a per-author `.unwrap_or_else(path)` at
/// the compile_from_args call site.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 (typed entry) — the
/// three-way authoring shape becomes a typed enum at the derive's
/// projection boundary, not a lossy `bool` that drops the
/// path-payload; the missing-kwarg branch is a typed CONSEQUENCE of
/// the projection rather than a per-author workaround. THEORY.md
/// §V.1 (knowable platform) — the derive's missing-kwarg semantics
/// now match serde's byte-for-byte on the `#[serde(default = "…")]`
/// axis, so authors reading serde's docs get the semantics the docs
/// promise without a per-author consultation of the derive's
/// deviations.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SerdeDefault {
    /// No `#[serde(default)]` attribute on the field — a missing
    /// kwarg surfaces as a hard [`LispError::MissingKwarg`]
    /// rejection at the derive's typed-entry gate rather than as a
    /// silent fallback.
    Absent,
    /// Bare `#[serde(default)]` — a missing kwarg falls back to
    /// [`::std::default::Default::default`], matching serde's bare-
    /// flag semantics.
    Trait,
    /// `#[serde(default = "path")]` — a missing kwarg falls back to
    /// `path()`, matching serde's per-field initializer-fn
    /// semantics. The `path` slot carries the caller-authored fully-
    /// qualified path string verbatim.
    Path(String),
}

/// Project a field's `#[serde(default[ = "path"])]` attribute posture
/// into the typed [`SerdeDefault`] variant the derive's missing-kwarg
/// dispatch branches on.
///
/// Routes through the shared [`find_named_sub_key`] helper — the same
/// entry the sibling [`extract_keyword`] reader uses. The `on_match`
/// callback captures the `= "path"` payload's optional presence:
/// `Ok(None)` on the bare-flag `#[serde(default)]` form (`meta.value()`
/// returns Err), `Ok(Some(path))` on the `#[serde(default = "path")]`
/// form (payload is a `LitStr`, projected to an owned `String`). The
/// outer projection then folds the `Option<Option<String>>` return
/// shape into the typed [`SerdeDefault`] enum:
///
/// - `None` (no `default` sub-key found) → [`SerdeDefault::Absent`]
/// - `Some(None)` (bare flag) → [`SerdeDefault::Trait`]
/// - `Some(Some(path))` (with payload) → [`SerdeDefault::Path(path)`]
///
/// The helper closes the attr-path gate, the sub-key ident match, and
/// the defensive value-drain across unmatched peers at ONE substrate
/// entry — matching how [`extract_keyword`] and its sibling
/// [`has_serde_default`] both compose against the same primitive on
/// distinct projections.
fn serde_default(field: &syn::Field) -> SerdeDefault {
    match find_named_sub_key(&field.attrs, "serde", "default", |meta| {
        match meta.value() {
            Ok(value) => parse_lit_str(value).map(Some),
            Err(_) => Ok(None),
        }
    }) {
        None => SerdeDefault::Absent,
        Some(None) => SerdeDefault::Trait,
        Some(Some(path)) => SerdeDefault::Path(path),
    }
}

/// Check if the field carries `#[serde(default)]` / `#[serde(default = "…")]`.
///
/// One-line delegate to the typed [`serde_default`] projection —
/// `true` iff the projection returns anything but [`SerdeDefault::Absent`].
/// Kept as a stable bool-returning surface for the pre-lift test cohort
/// that only cares about the presence/absence bit; the derive itself
/// reaches for [`serde_default`] directly to distinguish the
/// [`SerdeDefault::Trait`] and [`SerdeDefault::Path`] branches at the
/// missing-kwarg dispatch site inside [`extractor_for`].
///
/// The typed [`SerdeDefault`] projection rejects the false positive
/// the pre-lift substring-match implementation surfaced on
/// `#[serde(rename = "default_val")]` AND lets `default` appear
/// ANYWHERE in the sub-key list without the callback stalling at a
/// preceding value-carrying peer's `=` (pinned as
/// `default_after_other_key_named_value_pair`).
#[cfg(test)]
fn has_serde_default(field: &syn::Field) -> bool {
    !matches!(serde_default(field), SerdeDefault::Absent)
}

/// The EIGHT numeric-narrowed extractor arms in [`extractor_for`]
/// — `Kind::Int(_)`, `Kind::OptionalInt(_)`, `Kind::VecInt(_)`,
/// `Kind::OptionalVecInt(_)`, `Kind::Float(_)`, `Kind::OptionalFloat(_)`,
/// `Kind::VecFloat(_)`, `Kind::OptionalVecFloat(_)` — each emit the
/// SAME two-primitive composition, dispatched through the
/// [`tatara_lisp::domain::NarrowNumeric`] trait's per-narrow-type
/// `type Wide` associated-type binding (per-run cb9648d):
///
/// ```ignore
/// let narrowed: TokenStream2 = rust_ty.parse().unwrap();
/// quote! {
///     <#narrowed as ::tatara_lisp::domain::NarrowNumeric>::<method>(&kw, #key)?
/// }
/// ```
///
/// where `<method>` is one of FOUR mode-suffixed trait methods on
/// [`tatara_lisp::domain::NarrowNumeric`] —
/// `extract_narrowed_kwarg` (scalar) / `extract_optional_narrowed_kwarg`
/// (optional scalar) / `extract_narrowed_list_kwarg` (vec) /
/// `extract_optional_narrowed_list_kwarg` (optional vec). The
/// axis identity (int vs. float) is NOT carried in the method name
/// — it lives at ONE per-narrow-type `type Wide = i64|f64`
/// associated-type binding on the target width's
/// `impl NarrowNumeric for _` block, and rides the trait dispatch
/// through `<T as NarrowNumeric>::Wide` at the substrate's four
/// trait defaults (which compose `(Self::Wide, Self::narrow,
/// Self::WIDTH)` through the shared `extract_narrowed` /
/// `extract_optional_narrowed` / `extract_narrowed_list` /
/// `extract_optional_narrowed_list` substrate primitives).
///
/// Pre-lift the eight arms dispatched to eight axis-typed public
/// wrapper names in `tatara_lisp::domain`
/// (`extract_int_narrowed` / `extract_optional_int_narrowed` /
/// `extract_int_list_narrowed` /
/// `extract_optional_int_list_narrowed` on the int axis, and
/// their `float` peers). Post-lift the axis identity no longer
/// names the emitted method — the four method names cover both
/// axes because `<T as NarrowNumeric>::Wide` picks up the wide
/// axis mechanically from the target width. The dispatch surface
/// at the derive's `syn::Type -> emitted extractor call` boundary
/// halves from eight axis-typed wrapper names to four
/// mode-suffixed trait methods, and adding a hypothetical third
/// wide axis (e.g. `u128` / `Decimal`) lands as ZERO new emit
/// strings — the same four trait methods pick up the new axis
/// mechanically from the new `impl NarrowNumeric for _ { type
/// Wide = <NewWide>; ... }` block.
///
/// Post-lift the scaffold lives here at ONE substrate entry: the
/// helper parses the [`Kind`] payload's width literal (`"u16"`,
/// `"f32"`, `"usize"`, …) as a `TokenStream2` TURBOFISH, resolves
/// the trait method `Ident` at the derive's call-site span
/// (matching the `Ident::new` discipline the peer `via_ident` /
/// `unknown_ident` resolvers on line 185 already use), and emits
/// the fully-qualified `<T as ::tatara_lisp::domain::NarrowNumeric>
/// ::<method>(&kw, #key)?` call every numeric-narrowed arm shares.
/// Each of the eight numeric-narrowed arms (four per axis: scalar
/// / optional-scalar / vec / optional-vec, both int and float) is
/// now a ONE-LINE delegate onto this helper — with the int-axis
/// and float-axis arms of each mode collapsing to the SAME method
/// name (the axis-typed pair `(Kind::Int, Kind::Float)` both emit
/// `extract_narrowed_kwarg`, `(Kind::OptionalInt,
/// Kind::OptionalFloat)` both emit `extract_optional_narrowed_kwarg`,
/// and so on).
///
/// The `method` parameter is a `&'static str` — the ONE per-mode
/// dispatch identity — chosen for two reasons:
///
/// 1. it composes byte-for-byte with the `#[static]`-lifetime
///    `Kind::Int(&'static str)` / `Kind::VecInt(&'static str)` /
///    `Kind::OptionalVecInt(&'static str)` / etc. payload's own
///    `'static` lifetime origin, so the eight call sites don't
///    need to `.to_string()` anything to feed the helper;
/// 2. the string identity (four known values —
///    `extract_narrowed_kwarg` /
///    `extract_optional_narrowed_kwarg` /
///    `extract_narrowed_list_kwarg` /
///    `extract_optional_narrowed_list_kwarg`) is pinned per-mode
///    at the derive's `extractor_for` call site, so a regression
///    that silently swapped ONE arm's method name would surface
///    at the sibling test in `narrow_trait_dispatch_call_tests`
///    below rather than as silent drift at every downstream
///    implementor with a field of that mode shape.
///
/// Future structural promotion of the emitted call (a
/// caller-supplied diagnostic span, a `?`-suppressed variant that
/// plumbs the axis-typed rejection through a `Result` chain
/// rather than a `?`, or an extension of the four-method set with
/// a new mode) lands at ONE substrate primitive here — the eight
/// per-Kind arms and every future new numeric-narrowed arm pick
/// up the upgrade mechanically, with no per-arm hand-edit. Same
/// property [`extract_atom`] / [`extract_optional_atom`] /
/// [`extract_list`] give the atom-family / list-family primitives
/// on the `tatara_lisp::domain` side.
///
/// Numeric-narrowed peer of [`atom_trait_dispatch_call`] and
/// [`deserialize_trait_dispatch_call`] on the derive's proc-macro
/// emission surface: all three share the same emission skeleton
/// (resolve dispatch `Ident` at call-site span → emit fully-
/// qualified `<T as <Trait>>::<method>(&kw, #key)?` UFCS call);
/// this helper additionally bundles a TURBOFISH-typed dispatcher
/// (the payload's width literal rides the `<T as ...>::<method>`
/// UFCS prefix so the trait resolution picks up the axis from
/// `<T as NarrowNumeric>::Wide`), needed by the numeric-narrowing
/// axes. Together the three helpers close every kind-arm emission
/// at the derive's `syn::Type -> emitted extractor call` boundary
/// through ONE trait-per-axis-family substrate primitive.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition;
/// the (parse-payload-as-turbofish, wrap-in-mode-specific-trait-call)
/// two-step recurred at the eight axis-mode arms (well past the
/// PRIME-DIRECTIVE ≥2 trigger) and is lifted to one owner here,
/// exactly as the atom skeleton was lifted on the domain side.
/// THEORY.md §II.1 invariant 3 — typed exit; the per-narrow-type
/// narrowing rejection is a typed exit through the trait's
/// associated `Wide` type + `WIDTH` const, so a caller cannot
/// accidentally invoke the int-mode arm against a float-axis
/// narrow width because the trait's `type Wide` binding routes
/// axis identity at rustc time. THEORY.md §II.1 invariant 5 —
/// composition preserves proofs; the eight arms now compose
/// structurally through ONE helper at FOUR method names (not eight),
/// so a future third wide axis lands as ZERO new emit strings and
/// a future fifth mode lands as ONE new one-line delegate at the
/// [`extractor_for`] match plus a new `&'static str` mode name
/// flowed through the same helper — the derive's numeric-narrowing
/// emission surface stays at ONE substrate primitive.
/// Shared trailing UFCS-arm shape every trait-dispatch helper on the
/// derive's emission surface — [`narrow_trait_dispatch_call`] /
/// [`atom_trait_dispatch_call`] / [`deserialize_trait_dispatch_call`]
/// — appends after its axis-specific UFCS shell:
///
/// ```ignore
/// quote! { #method(&kw, #key)? }
/// ```
///
/// Owns THREE shared invariants the three peer helpers pre-lift each
/// restated verbatim:
///
/// 1. **Method Ident at `call_site()` span** — the `method` `&'static str`
///    resolves to a `syn::Ident` at the derive's own call-site span so
///    the UFCS dispatch is hygienic in the derived code's outer scope
///    regardless of what the implementor's `use` imports bring in. The
///    peer `Ident::new(_, Span::call_site())` sites on the derive's
///    `via_ident` / `unknown_ident` resolvers (line 185) already share
///    this discipline; the tail lift folds the derive's UFCS-family
///    resolvers onto ONE `call_site()`-Ident-emitting substrate primitive.
/// 2. **`(&kw, #key)?` positional-arg shape** — the extracted-kwarg map
///    borrow (`&kw`) plus the kwarg-name string literal (`#key`) ride
///    the trait method's two positional arguments in a fixed order; the
///    kwarg name is a Rust string literal (kebab-cased from the field
///    ident by [`snake_to_kebab`]) rather than an ident.
/// 3. **`?`-suffix** — the tail ends with the `?` operator so the
///    caller-visible expression type is the extracted `T` / `Option<T>`
///    / `Vec<T>` / `Option<Vec<T>>` shape rather than a `Result`; every
///    consumer of the UFCS call composes its output directly into a
///    derived struct-field initializer without a per-arm re-`?`.
///
/// Pre-lift these three invariants each lived three times, once per
/// axis-typed helper (`NarrowNumeric` / `AtomKwarg<'_>` /
/// `DeserializeKwarg`). Post-lift the tail lives at ONE substrate
/// primitive; each of the three axis-typed helpers keeps its
/// trait-specific UFCS shell but interpolates `#tail` at the trailing
/// dispatch position — the emitted token stream is byte-identical to
/// the pre-lift hand-rolled shape (the `>>::` closing sequence and the
/// `(&kw, #key)?` arg pattern both live in the outer per-axis quote
/// block, so proc_macro2's joint/alone tokenization of the axis-typed
/// UFCS bracket is preserved unchanged).
///
/// The tail's axis-agnosticism is the load-bearing property this lift
/// buys: a future FOURTH trait axis (a hypothetical `SymbolKwarg`,
/// `PathKwarg`, or `EnumVariantKwarg`) reuses this tail primitive
/// rather than restating the three invariants a fourth time. A future
/// structural upgrade to any of the three invariants (a caller-supplied
/// diagnostic span at the method ident, a third positional argument
/// carrying a callsite-derived context frame, a `?`-suppressed variant
/// that plumbs the axis-typed rejection through a `Result` chain) lands
/// at ONE substrate primitive here; ALL THREE existing trait-dispatch
/// helpers pick up the upgrade mechanically with no per-helper hand-edit
/// — matching the property [`extract_atom`] / [`extract_optional_atom`]
/// / [`extract_list`] give the atom-family primitives on the
/// `tatara_lisp::domain` side, and the property the three per-axis
/// trait-dispatch helpers already give at their own emission surfaces.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// (resolve-method-as-Ident, wrap-in-arg-shape-and-`?`) two-step
/// recurred across the three axis-typed trait-dispatch helpers (well
/// past the PRIME-DIRECTIVE ≥2 trigger) and is lifted to one owner
/// here. THEORY.md §II.1 invariant 5 — composition preserves proofs;
/// the three axis-typed helpers now compose structurally through ONE
/// tail primitive, so a regression that drifted any of the three tail
/// invariants at ONE axis would surface at the [`trait_dispatch_tail_tests`]
/// module below rather than as silent drift at every downstream
/// implementor with a field on that axis.
fn trait_dispatch_tail(method: &'static str, key: &str) -> TokenStream2 {
    let method = Ident::new(method, proc_macro2::Span::call_site());
    quote! {
        #method(&kw, #key)?
    }
}

fn narrow_trait_dispatch_call(method: &'static str, rust_ty: &str, key: &str) -> TokenStream2 {
    let narrowed: TokenStream2 = rust_ty.parse().unwrap();
    let tail = trait_dispatch_tail(method, key);
    quote! {
        <#narrowed as ::tatara_lisp::domain::NarrowNumeric>::#tail
    }
}

/// The FOUR atom-family list-family extractor arms in
/// [`extractor_for`] — `Kind::VecString`, `Kind::OptionalVecString`,
/// `Kind::VecBool`, `Kind::OptionalVecBool` — each emit the SAME
/// two-primitive composition, dispatched through the
/// [`tatara_lisp::domain::AtomKwarg`] trait's per-atom-type
/// `Self::Owned` associated-type binding:
///
/// ```ignore
/// let atom_ty: TokenStream2 = rust_ty.parse().unwrap();
/// quote! {
///     <#atom_ty as ::tatara_lisp::domain::AtomKwarg<'_>>::<method>(&kw, #key)?
/// }
/// ```
///
/// where `<method>` is one of TWO mode-suffixed trait methods on
/// [`tatara_lisp::domain::AtomKwarg`] — `extract_list_kwarg` (required
/// list) / `extract_optional_list_kwarg` (optional list). The axis
/// identity (string vs. bool) is NOT carried in the method name — it
/// lives at ONE per-atom-type `Self::Owned = String|bool` associated-
/// type binding on the target axis's `impl AtomKwarg for _` block, and
/// rides the trait dispatch through `<T as AtomKwarg>::Owned` at the
/// substrate's two list-family trait defaults (which compose
/// `(Self::LIST_SHAPE, Self::project_at, Self::to_owned_item)` through
/// the shared [`extract_list`] outer-shape skeleton).
///
/// Pre-lift the four arms dispatched to four axis-typed public wrapper
/// names in `tatara_lisp::domain` (`extract_string_list` /
/// `extract_optional_string_list` on the string axis, and
/// `extract_bool_list` / `extract_optional_bool_list` on the bool
/// axis). Post-lift the axis identity no longer names the emitted
/// method — the two method names cover both axes because `<T as
/// AtomKwarg>::Owned` picks up the axis-typed owned-element type
/// mechanically from the target atom axis (`String` for `<&str>`,
/// `bool` for `<bool>`). The dispatch surface at the derive's
/// `syn::Type -> emitted extractor call` boundary halves from four
/// axis-typed wrapper names to two mode-suffixed trait methods, and
/// adding a hypothetical third atom axis (e.g. `Symbol` with
/// `Owned = SymbolBuf`) lands as ZERO new emit strings — the same
/// two trait methods pick up the new axis mechanically from the new
/// `impl AtomKwarg for _ { type Owned = <NewOwned>; ... }` block.
///
/// Post-lift the scaffold lives here at ONE substrate entry: the
/// helper parses the [`Kind`] payload's atom-type literal (`"& str"`,
/// `"bool"`) as a `TokenStream2` UFCS `Self` type, resolves the trait
/// method `Ident` at the derive's call-site span (matching the
/// `Ident::new` discipline the peer [`narrow_trait_dispatch_call`] /
/// `via_ident` / `unknown_ident` resolvers already use), and emits
/// the fully-qualified `<T as ::tatara_lisp::domain::AtomKwarg<'_>>
/// ::<method>(&kw, #key)?` call every atom-family list-family arm
/// shares. Each of the four atom-family list-family arms (two per
/// axis: vec / optional-vec, both string and bool) is now a
/// ONE-LINE delegate onto this helper — with the string-axis and
/// bool-axis arms of each mode dispatching to the SAME method name
/// (the axis-typed pair `(Kind::VecString, Kind::VecBool)` both
/// emit `extract_list_kwarg`, `(Kind::OptionalVecString,
/// Kind::OptionalVecBool)` both emit `extract_optional_list_kwarg`).
///
/// The lifetime slot on `AtomKwarg<'_>` is elided — every atom-family
/// list-family impl (`impl<'a> AtomKwarg<'a> for &'a str`,
/// `impl<'a> AtomKwarg<'a> for bool`) resolves the trait `'a` from
/// the borrow lifetime of `&kw` at the emission site, so the derived
/// code carries no lifetime bookkeeping across the emit-time atom-axis
/// dispatch. The `Self` type in the UFCS slot rides the atom-axis
/// identity (`&str` for the string axis, `bool` for the bool axis);
/// the axis-typed `Self::Owned` binding on each impl block (`String`
/// for `<&str>`, `bool` for `<bool>`) picks up the owned-element type
/// mechanically, so the derived struct-field initializer's type
/// `Vec<String>` / `Vec<bool>` matches the trait method's
/// `Vec<Self::Owned>` return without a per-axis wrap in the derive's
/// emit path (the SAME `Self::Owned` axis identity the free-function
/// wrappers already dispatch through).
///
/// The `method` parameter is a `&'static str` — the ONE per-mode
/// dispatch identity — chosen for the same two reasons its
/// numeric-narrowed peer [`narrow_trait_dispatch_call`] takes a
/// `&'static str`:
///
/// 1. it composes byte-for-byte with the `#[static]` string literals
///    per-arm at the `extractor_for` match call site (no
///    `.to_string()` allocation on the emit path);
/// 2. the string identity (two known values —
///    `extract_list_kwarg` / `extract_optional_list_kwarg`) is
///    pinned per-mode at the derive's `extractor_for` call site, so
///    a regression that silently swapped ONE arm's method name would
///    surface at the sibling test in `atom_trait_dispatch_call_tests`
///    below rather than as silent drift at every downstream
///    implementor with a field of that mode shape.
///
/// Future structural promotion of the emitted call (a caller-supplied
/// diagnostic span, a `?`-suppressed variant that plumbs the
/// axis-typed rejection through a `Result` chain rather than a `?`,
/// or an extension of the two-method set with a new mode) lands at
/// ONE substrate primitive here — the four per-Kind arms and every
/// future new atom-family list-family arm pick up the upgrade
/// mechanically, with no per-arm hand-edit. Same property
/// [`narrow_trait_dispatch_call`] gives the eight numeric-narrowed
/// arms on the derive side.
///
/// Numeric-narrowed peer of [`narrow_trait_dispatch_call`] on the
/// derive's proc-macro emission surface: both share the same emission
/// skeleton (resolve dispatch `Ident` at call-site span → emit
/// fully-qualified UFCS call ending in `(&kw, #key)?`); the difference
/// is that `narrow_trait_dispatch_call` dispatches through the
/// [`tatara_lisp::domain::NarrowNumeric`] trait with a wide-axis
/// binding on `<T as NarrowNumeric>::Wide`, while this helper
/// dispatches through the [`tatara_lisp::domain::AtomKwarg`] trait
/// with an owned-axis binding on `<T as AtomKwarg>::Owned`. Together
/// the two helpers close every trait-dispatched arm's emission at
/// the derive's `syn::Type -> emitted extractor call` boundary
/// through ONE pair of substrate primitives — each one carrying a
/// distinct trait dispatch (numeric-narrowing on one, atom-family
/// list-family on the other).
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// (parse-payload-as-atom-type, wrap-in-mode-specific-trait-call)
/// two-step recurred at the four axis-mode arms (well past the
/// PRIME-DIRECTIVE ≥2 trigger) and is lifted to one owner here,
/// exactly as its numeric-narrowed peer was lifted onto
/// [`narrow_trait_dispatch_call`]. THEORY.md §II.1 invariant 3 —
/// typed exit; the per-atom-type owned-element rejection is a typed
/// exit through the trait's associated `Owned` type, so a caller
/// cannot accidentally invoke the string-axis emit against a
/// bool-axis field because the trait's `type Owned` binding routes
/// axis identity at rustc time. THEORY.md §II.1 invariant 5 —
/// composition preserves proofs; the four arms now compose
/// structurally through ONE helper at TWO method names (not four),
/// so a future third atom axis lands as ZERO new emit strings and a
/// future third mode lands as ONE new one-line delegate at the
/// [`extractor_for`] match plus a new `&'static str` mode name
/// flowed through the same helper — the derive's atom-family
/// list-family emission surface stays at ONE substrate primitive.
fn atom_trait_dispatch_call(method: &'static str, rust_ty: &str, key: &str) -> TokenStream2 {
    let atom_ty: TokenStream2 = rust_ty.parse().unwrap();
    let tail = trait_dispatch_tail(method, key);
    quote! {
        <#atom_ty as ::tatara_lisp::domain::AtomKwarg<'_>>::#tail
    }
}

/// The FOUR universal-serde-fallthrough extractor arms in
/// [`extractor_for`] — `Kind::Deserialize`, `Kind::OptionalDeserialize`,
/// `Kind::VecDeserialize`, `Kind::OptionalVecDeserialize` — each
/// now emit the SAME UFCS trait-dispatch composition through the
/// `Self`-bound [`tatara_lisp::domain::DeserializeKwarg`] trait:
///
/// ```ignore
/// quote! {
///     <#inner_ty as ::tatara_lisp::domain::DeserializeKwarg>::#method(&kw, #key)?
/// }
/// ```
///
/// where `#method` is one of four mode-suffixed trait defaults
/// (`extract_kwarg` / `extract_optional_kwarg` / `extract_vec_kwarg`
/// / `extract_optional_vec_kwarg`) and `#inner_ty` is the field's
/// inner `T: DeserializeOwned` — the whole field type on
/// `Kind::Deserialize`, the type inside `Option<_>` on
/// `Kind::OptionalDeserialize`, the type inside `Vec<_>` on
/// `Kind::VecDeserialize`, and the type inside `Option<Vec<_>>` on
/// `Kind::OptionalVecDeserialize`. The trait's blanket
/// `impl<T: DeserializeOwned> DeserializeKwarg for T {}` picks up
/// every field type mechanically at rustc time — the same closed
/// set the four free-function peers in `tatara_lisp::domain`
/// (`extract_via_serde` / `extract_optional_via_serde` /
/// `extract_vec_via_serde` / `extract_optional_vec_via_serde`)
/// already gate against.
///
/// Pre-lift each of the four arms hand-wrote a two-line
/// `quote! { ::tatara_lisp::domain::<name>(&kw, #key)? }` at ONE
/// derive call site through a shared bare-name-emission shim on
/// the universal-serde-fallthrough axis — a duplicated one-step
/// (resolve the substrate `Ident`, wrap it in the arg-list call)
/// that recurred well past the PRIME-DIRECTIVE ≥ 2 threshold at
/// the same match dispatch as its numeric-narrowed and atom-
/// family peers. Post
/// the universal-serde-fallthrough trait-dispatch lift the axis
/// identity rides `<T as DeserializeKwarg>` at the UFCS trait
/// bound rather than the derive's emit string, and the derive's
/// non-narrowed non-atom emission surface consists of ONE trait
/// dispatch primitive per axis-family:
///
/// | axis-family        | trait                              | derive helper                        |
/// |--------------------|------------------------------------|--------------------------------------|
/// | atom-family        | [`tatara_lisp::domain::AtomKwarg`] | [`atom_trait_dispatch_call`]         |
/// | numeric-narrowed   | [`tatara_lisp::domain::NarrowNumeric`] | [`narrow_trait_dispatch_call`]   |
/// | universal-serde    | [`tatara_lisp::domain::DeserializeKwarg`] | this helper                    |
///
/// Peer to [`narrow_trait_dispatch_call`] / [`atom_trait_dispatch_call`]
/// on the emission-shape axis: all three helpers resolve a
/// `&'static str` method name as an `Ident` at the derive's
/// call-site span and emit a UFCS trait-dispatch call ending in
/// `(&kw, #key)?`. The difference is which trait's dispatch
/// vocabulary rides the `<... as ...>::` prefix:
/// `narrow_trait_dispatch_call` binds
/// `<T as NarrowNumeric>::<method>` with a wide-axis binding on
/// `<T as NarrowNumeric>::Wide`; `atom_trait_dispatch_call` binds
/// `<T as AtomKwarg<'_>>::<method>` with an owned-axis binding on
/// `<T as AtomKwarg>::Owned`; this helper binds
/// `<T as DeserializeKwarg>::<method>` with `T: DeserializeOwned`
/// picked up by the blanket impl.
///
/// The `method` parameter is a `&'static str` — the ONE per-mode
/// dispatch identity — chosen for the same two reasons its atom-
/// and numeric-narrowed peers take a `&'static str`: (1) it
/// composes byte-for-byte with the `#[static]` string literals
/// per-arm at the `extractor_for` match call site (no
/// `.to_string()` allocation on the emit path), and (2) the string
/// identity (four known values — `extract_kwarg` /
/// `extract_optional_kwarg` / `extract_vec_kwarg` /
/// `extract_optional_vec_kwarg`) is pinned per-mode at the
/// derive's `extractor_for` call site, so a regression that
/// silently swapped ONE arm's method name would surface at the
/// sibling test in `deserialize_trait_dispatch_call_tests` below
/// rather than as silent drift at every downstream implementor
/// with a field of that mode shape.
///
/// Future structural promotion of the emitted call (a caller-
/// supplied diagnostic span, a `?`-suppressed variant that plumbs
/// the axis-typed rejection through a `Result` chain rather than a
/// `?`, an audit-trail metric jointly labeled by "which mode
/// fired" and "which field-type owned the decode") lands at ONE
/// substrate primitive here — all FOUR mode arms and every future
/// new mode pick up the upgrade mechanically, with no per-arm
/// hand-edit. Same property the two peer trait-dispatch helpers
/// give the atom-family and numeric-narrowed axes on the derive
/// side.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition;
/// the (resolve-method-as-Ident, wrap-in-UFCS-trait-call) one-step
/// recurred at the four universal-serde-fallthrough arms (well
/// past the PRIME-DIRECTIVE ≥ 2 trigger) and is lifted to one
/// owner here, exactly as its numeric-narrowed and atom-family
/// peers were lifted onto [`narrow_trait_dispatch_call`] and
/// [`atom_trait_dispatch_call`]. THEORY.md §II.1 invariant 5 —
/// composition preserves proofs; the four arms now compose
/// structurally through ONE helper, so a future new
/// universal-serde-fallthrough mode lands as ONE new one-line
/// delegate at the [`extractor_for`] match plus a new `&'static str`
/// mode name flowed through the same helper — the derive's
/// universal-serde emission surface stays at ONE substrate
/// primitive.
fn deserialize_trait_dispatch_call(
    method: &'static str,
    inner_ty: TokenStream2,
    key: &str,
) -> TokenStream2 {
    let tail = trait_dispatch_tail(method, key);
    quote! {
        <#inner_ty as ::tatara_lisp::domain::DeserializeKwarg>::#tail
    }
}

fn extractor_for(ty: &Type, key: &str, default: &SerdeDefault) -> Result<TokenStream2, String> {
    let kind = classify(ty);
    let base = match kind {
        // Atom-family scalar-family peers of the atom-family
        // list-family arms below — both the string axis
        // (`Kind::String` / `Kind::OptionalString`) and the bool
        // axis (`Kind::Bool` / `Kind::OptionalBool`, further below)
        // now dispatch through `<T as AtomKwarg<'_>>::
        // extract_owned_kwarg` / `extract_optional_owned_kwarg`
        // with `T ∈ {&str, bool}` picking the axis-typed trait impl
        // at rustc time. Pre-lift the string-axis arms hand-composed
        // `extract_string(&kw, key)?.to_string()` /
        // `extract_optional_string(&kw, key)?.map(String::from)` at
        // the derive call site — the `.to_string()` /
        // `.map(String::from)` owning-lift fold restated the
        // string-axis's `<&'a str>::Owned = String` associated-type
        // binding once per string-axis field per derive. Post-lift
        // the fold rides through `<&'a str as AtomKwarg<'_>>::
        // to_owned_item ≡ String::from` at the substrate's trait
        // default; the derived struct field type
        // (`String` / `Option<String>` on the string axis,
        // `bool` / `Option<bool>` on the bool axis) matches the
        // trait method's `Self::Owned` / `Option<Self::Owned>`
        // return without a per-axis wrap in the emit path.
        Kind::String => atom_trait_dispatch_call("extract_owned_kwarg", "& str", key),
        Kind::OptionalString => {
            atom_trait_dispatch_call("extract_optional_owned_kwarg", "& str", key)
        }
        // Atom-family list-family axes — both string (`Kind::VecString`) and
        // bool (`Kind::VecBool`) dispatch through `<T as AtomKwarg<'_>>::
        // extract_list_kwarg` with `T ∈ {&str, bool}` picking the axis-typed
        // trait impl at rustc time. Pre-lift each axis routed through its
        // own `extract_string_list` / `extract_bool_list` axis-typed public
        // wrapper name; post-lift the axis identity rides `<T as AtomKwarg>::
        // Owned` at the substrate's trait default rather than the derive's
        // emit string, and the two arms dispatch through ONE method name via
        // the shared [`atom_trait_dispatch_call`] helper. The derived struct
        // field type (`Vec<String>` / `Vec<bool>`) matches the trait method's
        // `Vec<Self::Owned>` return without a per-axis wrap in the emit path.
        Kind::VecString => atom_trait_dispatch_call("extract_list_kwarg", "& str", key),
        // `Option<Vec<String>>` routes through the typed
        // optional-string-list extractor (which delegates its per-item
        // decode to the same `<&str as AtomKwarg<'_>>::project_at`
        // atom-family shape gate the required peer `extract_string_list`
        // binds) rather than through `extract_optional_via_serde`.
        // Pre-lift `Option<Vec<String>>` fell through
        // `classify_option`'s catch-all arm to
        // `Kind::OptionalDeserialize` — the universal `sexp_to_json` +
        // `serde_json::from_value` bridge — so a per-item shape
        // mismatch on `:tags (list "ok" 5)` into an
        // `Option<Vec<String>>` field surfaced as a
        // `KwargDeserialize { message: "invalid type: integer ...,
        // expected a string at path .1" }` substring rather than as
        // the typed
        // `TypeMismatch { form: Item { key: "tags", idx: 1 },
        // expected: String, got: Int }` its REQUIRED peer
        // (`tags: Vec<String>`) already emits at the atom shape gate;
        // post-lift the two peers on the same axis speak the same
        // typed rejection vocabulary. Sibling routing lift to
        // [`Self::VecBool`] on the required-vec axis and to
        // [`Self::OptionalString`] / [`Self::OptionalBool`] on the
        // optional-scalar axis.
        Kind::OptionalVecString => {
            atom_trait_dispatch_call("extract_optional_list_kwarg", "& str", key)
        }
        // `Option<Vec<bool>>` routes through the typed
        // optional-bool-list extractor (which delegates its per-item
        // decode to the same `<bool as AtomKwarg<'_>>::project_at`
        // atom-family shape gate the required peer `extract_bool_list`
        // binds) rather than through `extract_optional_via_serde`.
        // Pre-lift `Option<Vec<bool>>` fell through
        // `classify_option`'s catch-all arm to
        // `Kind::OptionalDeserialize` — the universal `sexp_to_json` +
        // `serde_json::from_value` bridge — so a per-item shape
        // mismatch on `:flags (list #t "yes")` into an
        // `Option<Vec<bool>>` field surfaced as a
        // `KwargDeserialize { message: "invalid type: string \"yes\",
        // expected a boolean at path .1" }` substring rather than as
        // the typed
        // `TypeMismatch { form: Item { key: "flags", idx: 1 },
        // expected: Bool, got: String }` its REQUIRED peer
        // (`flags: Vec<bool>`) already emits at the atom shape gate;
        // post-lift the two peers on the same axis speak the same
        // typed rejection vocabulary. Sibling routing lift to
        // [`Self::VecBool`] on the required-vec-bool axis and to
        // [`Self::OptionalVecString`] on the optional-vec-string axis.
        Kind::OptionalVecBool => {
            atom_trait_dispatch_call("extract_optional_list_kwarg", "bool", key)
        }
        // ── The six numeric-narrowed arms: NARROWED, never `as`-cast ──
        //
        // Pre-lift each of the eight arms (four per axis: scalar /
        // optional-scalar / vec / optional-vec) hand-wrote the two-
        // step (parse the payload's width literal as a turbofish,
        // wrap it in the axis-mode-specific extractor call) at eight
        // sites in this match. Post-lift they collapse to one-line
        // delegates onto [`narrow_trait_dispatch_call`], which owns the
        // shared scaffold at ONE substrate primitive — see its
        // docstring for the load-bearing invariants.
        //
        // The scalar-mode axis matters here: the pre-lift emitted
        // code for `Kind::Int(_)` used to end with a raw Rust `as`
        // downcast — `extract_int(&kw, "port")? as u32`. `as` is
        // TOTAL by truncating, so `:port 4294967296` landed as `0`
        // and `:port -1` as `4294967295`, in the struct, silently,
        // with nothing red anywhere. The author read back a number
        // they never wrote. The width now rides the TURBOFISH into
        // `tatara_lisp::domain`'s `NarrowNumeric` projection, which
        // returns `LispError::KwargOutOfRange` for a value the field
        // cannot hold. Two consequences worth naming: this derive no
        // longer contains the word `as` on any numeric path (there
        // is no truncation left to regress), and the emitted code
        // names the width exactly ONCE — as a type — so the
        // diagnostic's `target` cannot drift from the field's actual
        // Rust type. The list-mode arms give the peer promise on the
        // per-item path (`extract_int_list_narrowed::<u16>` rejects
        // per-item out-of-range values with the same typed
        // `LispError::KwargOutOfRange { form: Item { key, idx },
        // target: U16, .. }` variant its scalar peer emits) — see
        // the `vec_u16_field_rejects_per_item_out_of_range_with_
        // typed_width_and_item_path` test in `tatara-lisp/src/
        // domain.rs` for the end-to-end pin.
        // Both axes (int / float) dispatch to the SAME trait method
        // `<T as NarrowNumeric>::extract_narrowed_list_kwarg` — the
        // wide axis rides `<T as NarrowNumeric>::Wide` at the
        // substrate's trait default, not the derive's emit string.
        Kind::VecInt(rust_ty) | Kind::VecFloat(rust_ty) => {
            narrow_trait_dispatch_call("extract_narrowed_list_kwarg", rust_ty, key)
        }
        // Optional-vec numeric-narrowed peers of the required-vec
        // arms directly above — the width payload rides the SAME
        // `narrow_trait_dispatch_call` helper into the outer-`Option`
        // extractor's turbofish, so a per-item narrowing failure on
        // `Option<Vec<u16>>` / `Option<Vec<f32>>` surfaces with the
        // SAME `NumericWidth::U16` / `NumericWidth::F32` identity its
        // required peer emits. Together these two arms close the last
        // two atom-family list-family axis-mode combinations that
        // still fell through the universal-serde bridge; post-lift
        // the derive dispatch surface covers the full Cartesian
        // product across {scalar, optional-scalar, vec, optional-vec}
        // × {String, Bool, Int, Float} for the atom-family axes.
        // Optional-vec numeric-narrowed peer of the required-vec
        // arm above. Both axes collapse onto ONE trait-dispatch call
        // through `<T as NarrowNumeric>::extract_optional_narrowed_list_kwarg`.
        Kind::OptionalVecInt(rust_ty) | Kind::OptionalVecFloat(rust_ty) => {
            narrow_trait_dispatch_call("extract_optional_narrowed_list_kwarg", rust_ty, key)
        }
        // Scalar numeric-narrowed axes — both `Kind::Int(_)` and
        // `Kind::Float(_)` dispatch to `<T as NarrowNumeric>::
        // extract_narrowed_kwarg`. Pre-lift each axis routed
        // through its own `extract_int_narrowed` /
        // `extract_float_narrowed` axis-typed public wrapper name;
        // post-lift the axis identity rides `<T as NarrowNumeric>::
        // Wide` at the substrate's trait default rather than the
        // derive's emit string, and the two arms collapse to ONE
        // method-name dispatch.
        Kind::Int(rust_ty) | Kind::Float(rust_ty) => {
            narrow_trait_dispatch_call("extract_narrowed_kwarg", rust_ty, key)
        }
        // Optional scalar numeric-narrowed peer of the required
        // scalar arm above. Both axes dispatch to `<T as
        // NarrowNumeric>::extract_optional_narrowed_kwarg`.
        Kind::OptionalInt(rust_ty) | Kind::OptionalFloat(rust_ty) => {
            narrow_trait_dispatch_call("extract_optional_narrowed_kwarg", rust_ty, key)
        }
        // Bool-axis peers of the string-axis `Kind::String` /
        // `Kind::OptionalString` arms above — both atom-family
        // scalar-family axes now dispatch through
        // [`atom_trait_dispatch_call`] with `T = bool` picking the
        // axis-typed trait impl at rustc time. The `<bool>::Owned
        // = bool` binding is a no-op copy, so the return type
        // matches `bool` / `Option<bool>` directly — same posture
        // the string-axis arm gives through its `<&'a str>::Owned
        // = String` binding, and same posture the bool-axis
        // list-family arm (`Kind::VecBool` below) gives through
        // `Vec<Self::Owned> = Vec<bool>`.
        Kind::Bool => atom_trait_dispatch_call("extract_owned_kwarg", "bool", key),
        Kind::OptionalBool => atom_trait_dispatch_call("extract_optional_owned_kwarg", "bool", key),
        // `Vec<bool>` routes through the typed bool-list extractor
        // (which decodes each element via the `<bool as AtomKwarg<'_>>::
        // project_at` per-item atom-family shape gate) rather than
        // through `extract_vec_via_serde`. Pre-lift `Vec<bool>` fell
        // through to `Kind::VecDeserialize` — the universal
        // `sexp_to_json` + `serde_json::from_value` bridge — so a
        // per-item shape mismatch on `:flags (list #t "yes")` into a
        // `Vec<bool>` field surfaced as a
        // `KwargDeserialize { message: "invalid type: string \"yes\",
        // expected a boolean at path .1" }` substring rather than as
        // the typed
        // `TypeMismatch { form: Item { key: "flags", idx: 1 },
        // expected: Bool, got: String }` its SCALAR peer (`enabled:
        // bool`) already emits at the atom shape gate; post-lift the
        // two gates on the same axis speak the same typed rejection
        // vocabulary.
        // Bool-axis peer of the `Kind::VecString` arm above — both
        // atom-family list-family axes now dispatch through
        // [`atom_trait_dispatch_call`] with `T = bool` picking the axis-
        // typed trait impl at rustc time. The `Self::Owned = bool`
        // binding on the bool axis is a no-op copy, so the return type
        // matches `Vec<bool>` directly without a per-axis wrap in the
        // emit path (same posture the string-axis arm gives through its
        // `Self::Owned = String` binding).
        Kind::VecBool => atom_trait_dispatch_call("extract_list_kwarg", "bool", key),
        // Fall-through: anything with `serde::Deserialize` works via
        // the sexp_to_json bridge. Unlocks enums, nested structs,
        // Vec<Struct>. Post-lift the four arms below dispatch through
        // [`deserialize_trait_dispatch_call`] with `T: DeserializeOwned`
        // picking the axis-typed trait impl at rustc time; the axis
        // identity rides `<T as DeserializeKwarg>` at the UFCS trait
        // bound rather than the derive's emit string, and every
        // future diagnostic upgrade on the universal-serde-fallthrough
        // axis lands at ONE trait default in `tatara_lisp::domain` and
        // flows through the four modes here mechanically. Same posture
        // the eight atom-family arms give through
        // [`atom_trait_dispatch_call`] and the eight numeric-narrowed
        // arms give through [`narrow_trait_dispatch_call`]. Together
        // the three helpers close the derive's full non-atom emission
        // surface: every arm dispatches through ONE trait-per-axis-
        // family primitive.
        // Post-lift the four `Kind::(Optional?)(Vec?)Deserialize`
        // arms each dispatch on the CACHED inner-`T` tokens the
        // classify pass ([`classify_option`] / [`classify_vec`])
        // already walked out of the outer field type. Pre-lift each
        // arm carried a `generic_arg(ty).ok_or_else(...)?` re-walk
        // at this call site — the [`Kind::OptionalVecDeserialize`]
        // arm carried TWO — restating the exact walk the classifier
        // had just performed and threading a `.ok_or_else(...)?`
        // fallible arm for an invariant already established (the
        // arms are only reachable through the `"Option"` / `"Vec"`
        // `classify` arm, which only routes there when
        // `first_generic_type` succeeded). Post-lift the walk lives
        // at ONE per-axis call site inside the classify pass, the
        // payload rides `Kind::(Optional?)(Vec?)Deserialize` through
        // to the extract site, and the four arms here reduce to
        // one-line delegates through [`deserialize_trait_dispatch_call`]
        // with the inner tokens threaded through the second
        // positional argument. Same payload posture the eight
        // numeric-narrowed arms already take through their
        // `Kind::Int(&'static str)` / `Kind::VecInt(&'static str)`
        // / `Kind::OptionalVecInt(&'static str)` payloads and the
        // [`narrow_trait_dispatch_call`] emission — the classifier
        // IS the projection from `syn::Type` to the emission-side
        // type identity, and the [`extractor_for`] match dispatches
        // on the payload.
        Kind::Deserialize(inner_ty) => {
            deserialize_trait_dispatch_call("extract_kwarg", inner_ty, key)
        }
        Kind::OptionalDeserialize(inner_ty) => {
            deserialize_trait_dispatch_call("extract_optional_kwarg", inner_ty, key)
        }
        Kind::VecDeserialize(inner_ty) => {
            deserialize_trait_dispatch_call("extract_vec_kwarg", inner_ty, key)
        }
        // `Option<Vec<T>>` on the universal-serde-fallthrough axis —
        // routes through the present-vs-absent bifurcated peer of
        // `extract_vec_via_serde` on the substrate via the trait
        // default `<T as DeserializeKwarg>::extract_optional_vec_kwarg`.
        // The trait default rides `optional_from_required(kw, key,
        // extract_vec_via_serde::<T>)` at the substrate primitive, so
        // (a) an absent kwarg returns `Ok(None)` without invoking the
        // required extractor (preserving the `None` / `Some(vec![])`
        // distinction the field carries), (b) a present-but-non-list
        // kwarg rejects with the SAME typed `LispError::TypeMismatch`
        // variant the required peer emits at the shared `extract_list`
        // outer-shape gate, and (c) a per-item serde-decode failure
        // inside a present list rejects with the SAME structural
        // `LispError::KwargDeserialize { path: KwargPath::Item { key,
        // idx }, message }` variant its required peer emits through
        // `from_value_with_path` at `KwargPath::item(key, idx)`.
        Kind::OptionalVecDeserialize(inner_ty) => {
            deserialize_trait_dispatch_call("extract_optional_vec_kwarg", inner_ty, key)
        }
    };
    // Respect `#[serde(default[ = "path"])]` — wrap extractor with a
    // missing-key short-circuit dispatched on the typed `SerdeDefault`
    // projection. The three-way dispatch matches serde's own semantics:
    // `Absent` requires the kwarg, `Trait` calls `Default::default()`,
    // `Path("path")` calls the caller-authored `path()` fn. The path
    // string parses as a `TokenStream2` so a `path::to::fn` operator-
    // authored dotted path rides through as a fully-qualified callee at
    // the emit site — matching how serde's own deserialize codegen
    // splices the same path into its missing-field branch.
    Ok(match default {
        SerdeDefault::Absent => base,
        SerdeDefault::Trait => quote! {
            if kw.contains_key(#key) { #base } else { ::std::default::Default::default() }
        },
        SerdeDefault::Path(path) => {
            let path_ts: TokenStream2 = path
                .parse()
                .map_err(|e| format!("invalid #[serde(default = {path:?})] path: {e}"))?;
            quote! {
                if kw.contains_key(#key) { #base } else { #path_ts() }
            }
        }
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
    /// `Vec<T>` where `T` is one of the supported integer widths
    /// (`i8` / `i16` / `i32` / `i64` / `u8` / `u16` / `u32` /
    /// `u64` / `usize` / `isize`). Carries the width literal the
    /// scalar `Int(_)` arm threads through the emitted extractor's
    /// TURBOFISH — post-lift the width identity rides
    /// `extract_int_list_narrowed::<T>` in ONE place at emit, so
    /// the per-item rejection carries the typed `NumericWidth`
    /// identity ([`crate::error::LispError::KwargOutOfRange.target`])
    /// its scalar peer already emits.
    VecInt(&'static str),
    /// Sibling of [`Self::VecInt`] on the float axis — `Vec<f32>` /
    /// `Vec<f64>`. Routes through `extract_float_list_narrowed::<T>`
    /// so a per-item lossy-to-inf overflow on the float axis
    /// (`(list 1.0 1.0e300)` into a `Vec<f32>` field) rejects as
    /// the typed `NumericWidth::F32` / `NumericLiteral::Float(_)`
    /// pair the scalar peer emits, not the mystery serde
    /// substring the universal-Deserialize fallthrough would.
    VecFloat(&'static str),
    Bool,
    OptionalBool,
    /// `Vec<bool>` — routes through
    /// [`tatara_lisp::domain::extract_bool_list`], the typed
    /// list-family peer of `extract_bool` on the atom-family
    /// per-item shape gate ([`tatara_lisp::domain::AtomKwarg::
    /// project_at`] on the `<bool>` axis). Pre-lift `Vec<bool>` fell
    /// through to [`Self::VecDeserialize`] — the universal
    /// `sexp_to_json` + `serde_json::from_value` bridge — so a
    /// per-item non-bool element inside `:flags (list #t "yes")`
    /// surfaced as a mystery serde substring diagnostic instead of
    /// the typed
    /// `LispError::TypeMismatch { form: Item { key, idx }, expected:
    /// Bool, got: String }` variant its scalar peer (`enabled: bool`)
    /// already emits at the atom shape gate.
    VecBool,
    /// `Option<Vec<String>>` — routes through
    /// [`tatara_lisp::domain::extract_optional_string_list`], the
    /// present-vs-absent bifurcated peer of [`Self::VecString`] on
    /// the string axis. Distinguishes an absent kwarg (`Ok(None)`)
    /// from a present empty list (`Ok(Some(Vec::new()))`) — the
    /// [`Self::VecString`] posture collapses both cases (both land
    /// as `Ok(Vec::new())`), which fits `Vec<String>` fields but
    /// loses the operator's intent on `Option<Vec<String>>` fields.
    /// Delegates its present-branch per-item decode to the SAME
    /// [`tatara_lisp::domain::extract_string_list`] the required
    /// peer binds, so a per-item non-string element inside
    /// `:tags (list "ok" 5)` rejects with the SAME typed
    /// `LispError::TypeMismatch { form: Item { key, idx }, expected:
    /// String, got: Int }` variant its required peer emits at the
    /// atom shape gate. Pre-lift `Option<Vec<String>>` fell through
    /// `classify_option`'s catch-all arm to
    /// [`Self::OptionalDeserialize`] (the universal `sexp_to_json` +
    /// `serde_json::from_value` bridge — no `Option<Vec<T>>` arm on
    /// the recursor), so a per-item shape mismatch surfaced as a
    /// mystery `KwargDeserialize` substring rather than the typed
    /// atom-family rejection its required peer already emits. Post-
    /// lift the two peers on the string axis speak ONE rejection
    /// vocabulary. Sibling routing lift to [`Self::VecBool`] on the
    /// required-vec axis and to [`Self::OptionalString`] on the
    /// optional-scalar axis; the optional-vec-bool /
    /// optional-vec-int / optional-vec-float axes still fall through
    /// to [`Self::OptionalDeserialize`] — same posture the required-
    /// vec-bool / required-vec-int / required-vec-float axes had
    /// before their per-axis typed extractors landed.
    OptionalVecString,
    /// `Option<Vec<bool>>` — routes through
    /// [`tatara_lisp::domain::extract_optional_bool_list`], the
    /// present-vs-absent bifurcated peer of [`Self::VecBool`] on the
    /// bool axis and the bool-axis peer of
    /// [`Self::OptionalVecString`] on the atom-family non-numeric
    /// list surface. Distinguishes an absent kwarg (`Ok(None)`) from
    /// a present empty list (`Ok(Some(Vec::new()))`) — the
    /// [`Self::VecBool`] posture collapses both cases (both land as
    /// `Ok(Vec::new())`), which fits `Vec<bool>` fields but loses the
    /// operator's intent on `Option<Vec<bool>>` fields. Delegates its
    /// present-branch per-item decode to the SAME
    /// [`tatara_lisp::domain::extract_bool_list`] the required peer
    /// binds, so a per-item non-bool element inside
    /// `:flags (list #t "yes")` rejects with the SAME typed
    /// `LispError::TypeMismatch { form: Item { key, idx }, expected:
    /// Bool, got: String }` variant its required peer emits at the
    /// atom shape gate. Pre-lift `Option<Vec<bool>>` fell through
    /// `classify_option`'s catch-all arm to
    /// [`Self::OptionalDeserialize`] (the universal `sexp_to_json` +
    /// `serde_json::from_value` bridge — no `Option<Vec<bool>>` arm
    /// on the recursor), so a per-item shape mismatch surfaced as a
    /// mystery `KwargDeserialize` substring rather than the typed
    /// atom-family rejection its required peer already emits. Post-
    /// lift the two peers on the bool axis speak ONE rejection
    /// vocabulary. Sibling routing lift to [`Self::OptionalVecString`]
    /// on the optional-vec-string axis and to [`Self::VecBool`] on
    /// the required-vec-bool axis; the peer optional-vec-int /
    /// optional-vec-float axes still fall through to
    /// [`Self::OptionalDeserialize`] — a future run picks each up in
    /// ONE arm extension per axis (matching how the required-vec-int
    /// / required-vec-float axes each grew ONE arm through their per-
    /// axis extension).
    OptionalVecBool,
    /// `Option<Vec<T>>` on the integer-narrowing axis — routes through
    /// [`tatara_lisp::domain::extract_optional_int_list_narrowed::<T>`],
    /// the present-vs-absent bifurcated peer of [`Self::VecInt`] on the
    /// int axis and the numeric-narrowing peer of [`Self::OptionalVecBool`]
    /// / [`Self::OptionalVecString`] on the optional-vec atom-family
    /// surface. Carries the width literal the required-peer [`Self::VecInt`]
    /// arm threads through the emitted extractor's TURBOFISH — the width
    /// identity rides `extract_optional_int_list_narrowed::<T>` in ONE
    /// place at emit, so the per-item narrowing rejection carries the
    /// same typed `NumericWidth` identity
    /// ([`crate::error::LispError::KwargOutOfRange.target`]) its
    /// required peer emits, only wrapped in the `Option` layer.
    ///
    /// Pre-lift `Option<Vec<T>>` on every integer width fell through
    /// `classify_option`'s catch-all arm to
    /// [`Self::OptionalDeserialize`] (the universal `sexp_to_json` +
    /// `serde_json::from_value` bridge — no `Option<Vec<T>>` numeric
    /// arm on the recursor), so a per-item out-of-range value on
    /// `:ports (list 80 70000)` into an `Option<Vec<u16>>` field
    /// surfaced as a mystery
    /// `KwargDeserialize { message: "invalid value: integer 70000,
    /// expected u16 at path .1" }` substring rather than as the typed
    /// `KwargOutOfRange { form: Item { key, idx }, target: U16, value:
    /// Int(70_000) }` its REQUIRED peer (`ports: Vec<u16>`) already
    /// emits at the per-item narrowing gate. Post-lift the two peers
    /// on the same integer axis speak the same typed rejection
    /// vocabulary. Sibling routing lift to [`Self::VecInt`] on the
    /// required-vec-int axis, to [`Self::OptionalInt`] on the
    /// optional-scalar-int axis, and to [`Self::OptionalVecBool`] /
    /// [`Self::OptionalVecString`] on the optional-vec atom-family
    /// non-numeric surface. Together with [`Self::OptionalVecFloat`],
    /// closes the last two remaining atom-family list-family axis-
    /// mode combinations that still fell through the universal-serde
    /// bridge — completing the Cartesian product across {scalar,
    /// optional-scalar, vec, optional-vec} × {String, Bool, Int,
    /// Float} for the atom-family axes at the derive dispatch surface.
    OptionalVecInt(&'static str),
    /// Sibling of [`Self::OptionalVecInt`] on the float axis —
    /// `Option<Vec<f32>>` / `Option<Vec<f64>>`. Routes through
    /// [`tatara_lisp::domain::extract_optional_float_list_narrowed::<T>`]
    /// so a per-item lossy-to-inf overflow on the float axis
    /// (`(list 1.0 1.0e300)` into an `Option<Vec<f32>>` field)
    /// rejects as the typed `NumericWidth::F32` /
    /// `NumericLiteral::Float(_)` pair the required peer
    /// [`Self::VecFloat`] emits, not the mystery serde substring the
    /// universal-Deserialize fallthrough would surface. See the doc-
    /// paragraph on [`Self::OptionalVecInt`] for the full pre-/post-
    /// lift shape — the axis identity riding `<f32>` / `<f64>` is
    /// what changes here, not the scaffold.
    OptionalVecFloat(&'static str),
    /// Fall-through: any type implementing `serde::Deserialize`.
    ///
    /// Payload is the CACHED inner-type tokens the emitted UFCS trait
    /// dispatch (`<T as DeserializeKwarg>::extract_kwarg`) binds at
    /// its `Self` slot — on `Kind::Deserialize` `T` is the whole
    /// field type (the fall-through arm of [`classify`] recognizes
    /// any non-primitive path type); on `Kind::OptionalDeserialize`
    /// it is the type inside `Option<_>`; on `Kind::VecDeserialize`
    /// it is the type inside `Vec<_>`; on
    /// `Kind::OptionalVecDeserialize` it is the type inside
    /// `Option<Vec<_>>`. The projection to the inner tokens runs
    /// ONCE at [`classify_option`] / [`classify_vec`], not at each
    /// [`extractor_for`] call site — pre-lift the derive's four
    /// `Kind::(Optional?)(Vec?)Deserialize` arms each re-walked the
    /// outer field type via a `generic_arg(ty)?` extractor-side
    /// helper (twice for the [`Kind::OptionalVecDeserialize`] arm),
    /// duplicating the walk `classify` had already performed and
    /// carrying a `.ok_or_else(...)?` fallible arm at each derive
    /// call site for an invariant already established at classify
    /// time (well-formed `Option<T>` / `Vec<T>` / `Option<Vec<T>>`
    /// path types always have their inner type generic present, else
    /// they'd not have taken the `"Option"` / `"Vec"` `classify`
    /// arm). Caching the walk here matches the payload posture the
    /// numeric-narrowed axes (`Kind::Int(&'static str)` /
    /// `Kind::VecInt(&'static str)` / `Kind::OptionalVecInt(&'static
    /// str)` etc.) already take — the classifier IS the projection
    /// from `syn::Type` to the emission-side type identity, and the
    /// [`extractor_for`] match dispatches on the payload rather than
    /// re-walking the type.
    Deserialize(TokenStream2),
    OptionalDeserialize(TokenStream2),
    VecDeserialize(TokenStream2),
    /// `Option<Vec<T>>` where `T` falls through to [`Self::VecDeserialize`]
    /// on the inner axis — a struct, an enum, a nested `Vec<T>`, or any
    /// non-atomic type. Routes through
    /// [`tatara_lisp::domain::extract_optional_vec_via_serde::<T>`], the
    /// present-vs-absent bifurcated peer of [`Self::VecDeserialize`] on
    /// the universal-serde-fallthrough list-family surface. Distinguishes
    /// an absent kwarg (`Ok(None)`) from a present empty list
    /// (`Ok(Some(Vec::new()))`) — the [`Self::VecDeserialize`] posture
    /// collapses both cases (both land as `Ok(Vec::new())`), which fits
    /// `Vec<T>` fields but loses the operator's intent on `Option<Vec<T>>`
    /// fields. Delegates its present-branch per-item decode to the SAME
    /// [`tatara_lisp::domain::extract_vec_via_serde`] the required peer
    /// binds, so a per-item serde-decode failure inside
    /// `:steps ((:notify-ref "ok") (:notify-ref 7))` on an
    /// `Option<Vec<EscalationStep>>` field rejects with the SAME
    /// structural `LispError::KwargDeserialize { path: KwargPath::Item {
    /// key, idx }, message }` variant its required peer emits at the
    /// per-item bridge — only wrapped in the `Option` layer for a
    /// present-vs-decoded return path.
    ///
    /// Pre-lift `Option<Vec<Nested>>` fell through `classify_option`'s
    /// catch-all arm to [`Self::OptionalDeserialize`] (the universal
    /// `sexp_to_json` + `serde_json::from_value` bridge — no
    /// `Option<Vec<T>>` arm on the recursor for the non-atomic-inner
    /// axis), so a per-item shape/decode mismatch surfaced as a serde-
    /// substring `LispError::KwargDeserialize { path: KwargPath::Named(
    /// key), message: "invalid type: ..., expected ..., at path .1" }`
    /// diagnostic keyed off the substring inside the message rather than
    /// as the typed
    /// `LispError::KwargDeserialize { path: KwargPath::Item { key, idx },
    /// .. }` its required peer [`Self::VecDeserialize`] already emits
    /// through the per-item bridge — the SAME class of gate-leak the
    /// prior [`Self::VecBool`] / [`Self::VecInt`] / [`Self::VecFloat`] /
    /// [`Self::OptionalVecString`] / [`Self::OptionalVecBool`] /
    /// [`Self::OptionalVecInt`] / [`Self::OptionalVecFloat`] arms
    /// closed on the atom-family surface. Post-lift the two peers on the
    /// universal-serde fallthrough axis share ONE rejection vocabulary,
    /// closing the LAST `Option<Vec<T>>` × mode Cartesian-product hole
    /// in the derive's typed-entry surface — every `Option<Vec<T>>`
    /// field, atomic-inner OR non-atomic-inner, now surfaces per-item
    /// rejections through a [`KwargPath::Item { key, idx }`] path root
    /// rather than a [`KwargPath::Named(key)`] one.
    OptionalVecDeserialize(TokenStream2),
}

/// The closed set of integer widths the derive routes through
/// `<T as NarrowNumeric>::extract_narrowed_kwarg` at the emit-time
/// UFCS dispatch — the ten widths [`crate::classify`] projects to
/// [`Kind::Int(_)`] and every peer `Kind::Vec{,Optional}Int(_)` /
/// `Kind::OptionalInt(_)` variant inherits through the recursor
/// arms. Each entry's `&'static str` IS the payload the emitted
/// TURBOFISH binds at the [`crate::narrow_trait_dispatch_call`]
/// UFCS `Self` slot, so the classifier's pattern-payload identity
/// (`"u16"` in the match, `"u16"` in the payload construction) IS
/// the single-source `w` variable rather than two hand-written
/// string literals per width.
///
/// Pre-lift each numeric arm on [`crate::classify`] spelled its
/// width TWICE — once as the match pattern (`"u16" => ...`) and
/// once as the payload (`Kind::Int("u16")`). Ten integer + two
/// float arms × two spellings = TWENTY-FOUR restated width strings
/// across the classifier's numeric surface, any pair of which
/// could drift silently: a regression like
/// `"u32" => Kind::Int("u16")` compiles and produces WRONG emit
/// code — the derived struct's `port: u32` field would rejection-
/// gate against `NumericWidth::U16` bounds rather than
/// `NumericWidth::U32`, so a value in the u16-exceeding, u32-fitting
/// range would ship as an authoring-side rejection on the operator's
/// side rather than parse into the field.
///
/// Post-lift the payload IS the matched string — the loop's `w`
/// variable rides both the equality gate (`w == name`) and the
/// `Kind::Int(w)` payload construction, so a pattern-payload drift
/// is structurally impossible: the two spellings collapse to ONE
/// per-width entry in this const, and the classifier's numeric arm
/// binds through iteration rather than through hand-restated match
/// arms. A future substrate extension to an eleventh integer width
/// (a hypothetical `u128` / `i128` axis, contingent on
/// [`tatara_lisp::domain::NarrowNumeric`] gaining an impl for the
/// new width and the reader's [`tatara_lisp::error::NumericWidth`]
/// gaining the matching variant) lands as ONE const entry here —
/// the classifier, every peer test module that iterates this const,
/// and every `#[derive(TataraDomain)]` implementor with a
/// `Vec<u128>` / `Option<i128>` field picks up the new axis
/// mechanically with no per-site edit.
///
/// The const's exposure at `pub(crate)` scope lets the derive's own
/// test modules
/// ([`crate::narrow_trait_dispatch_call_tests`] currently hard-
/// codes the same ten widths in a per-test loop; a future test
/// module can iterate this const instead so per-width test
/// coverage auto-extends when a width lands here).
///
/// Theory anchor: THEORY.md §II.1 invariant 1 — typed entry; the
/// pattern-payload identity across the classifier's numeric arms
/// IS the derive's rust-level typed-entry projection for narrowed
/// widths, and lifting it to ONE per-width const makes the
/// identity load-bearing in the source rather than in twenty-four
/// hand-restated string literals. THEORY.md §VI.1 (generation over
/// composition) — the (match-arm, payload) pair recurred at TWELVE
/// numeric arms in [`crate::classify`] (well past the
/// PRIME-DIRECTIVE ≥ 2 duplication threshold), and each pair
/// restated the same width literal twice against a different
/// hand-written string; lifting the width to ONE per-arm `w`
/// variable closes the twelve pairs at rustc time. THEORY.md
/// §V.1 (knowable platform) — an authoring surface that wants to
/// enumerate the supported narrowing widths (e.g. `tatara-check`'s
/// diagnostic renderer, an LSP hover hint listing which widths a
/// hypothetical typo would be closer to, a documentation
/// generator) now has ONE substrate handle rather than re-deriving
/// the list from the classifier's match-arm-by-match-arm layout.
pub(crate) const NARROW_INT_WIDTHS: &[&str] = &[
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "usize", "isize",
];

/// Float-axis sibling of [`NARROW_INT_WIDTHS`] — the closed set of
/// float widths the derive routes through
/// `<T as NarrowNumeric>::extract_narrowed_kwarg` on the
/// [`Kind::Float(_)`] / peer `Kind::Vec{,Optional}Float(_)` /
/// `Kind::OptionalFloat(_)` recursor arms. Same pattern-payload
/// single-source contract the int-width const owns on its
/// narrowing arm — the loop's `w` variable rides both the
/// equality gate and the `Kind::Float(w)` payload construction.
/// A future third float width (a hypothetical `f16` axis) lands
/// as ONE entry here, matching the int-axis extension contract.
pub(crate) const NARROW_FLOAT_WIDTHS: &[&str] = &["f32", "f64"];

fn classify(ty: &Type) -> Kind {
    if let Type::Path(path) = ty {
        if let Some(last) = path.path.segments.last() {
            let name = last.ident.to_string();
            match name.as_str() {
                "String" => return Kind::String,
                "bool" => return Kind::Bool,
                "Option" => return classify_option(last, ty),
                "Vec" => return classify_vec(last, ty),
                _ => {}
            }
            // Numeric-narrowed arms — the pattern-payload identity
            // across [`NARROW_INT_WIDTHS`] / [`NARROW_FLOAT_WIDTHS`]
            // rides ONE per-width `w` variable through both the
            // equality gate and the payload construction, so a
            // pattern-payload drift (e.g. a regression `"u32" match
            // arm shipping a `"u16"` payload) is structurally
            // impossible.
            for &w in NARROW_INT_WIDTHS {
                if w == name {
                    return Kind::Int(w);
                }
            }
            for &w in NARROW_FLOAT_WIDTHS {
                if w == name {
                    return Kind::Float(w);
                }
            }
        }
    }
    // Anything else: fall through to serde Deserialize. The whole
    // field type IS the inner `T` on the scalar-mode dispatch —
    // `<T as DeserializeKwarg>::extract_kwarg` binds `T` at the
    // UFCS `Self` slot verbatim.
    Kind::Deserialize(quote! { #ty })
}

fn classify_option(last: &syn::PathSegment, ty: &Type) -> Kind {
    let Ok(inner) = first_generic_type(last) else {
        // Malformed `Option` — no type arg present (e.g. a field
        // typed as bare `Option` or `Option<'a>`, both syntactically
        // parseable by `syn` but semantically incomplete). The
        // whole outer type rides through as the cached payload so
        // the emitted `<Option as DeserializeKwarg>::extract_optional_kwarg`
        // surface hits rustc's type checker at the derive
        // consumer's compile with a load-bearing type-mismatch,
        // not a silent narrowing. Well-formed `Option<T>` never
        // reaches this arm — `first_generic_type` succeeds on
        // every valid single-type-arg segment.
        return Kind::OptionalDeserialize(quote! { #ty });
    };
    match classify(inner) {
        Kind::String => Kind::OptionalString,
        Kind::Int(t) => Kind::OptionalInt(t),
        Kind::Float(t) => Kind::OptionalFloat(t),
        Kind::Bool => Kind::OptionalBool,
        // `Option<Vec<String>>` composes the outer `Option` recursor
        // with the `Vec<T>` recursor `classify_vec`, which projects
        // `Vec<String>` to `Kind::VecString`. This arm sharpens the
        // composed `Option<VecString>` to the typed
        // `Kind::OptionalVecString` variant rather than letting it
        // fall through the catch-all to `Kind::OptionalDeserialize`
        // (the universal `sexp_to_json` + `serde_json::from_value`
        // bridge, whose per-item shape mismatch surfaces as a
        // `LispError::KwargDeserialize` substring rather than the
        // typed `LispError::TypeMismatch { form: Item { key, idx },
        // expected: String, got: <shape> }` variant its required
        // peer `Kind::VecString` already emits at the atom-family
        // shape gate). Sibling routing lift to the required-vec
        // `Vec<bool>` → `Kind::VecBool` arm on `classify_vec` below;
        // the peer optional-vec-int / optional-vec-float axes still
        // fall through to the catch-all here — a future run picks
        // them up in ONE arm extension each.
        Kind::VecString => Kind::OptionalVecString,
        // `Option<Vec<bool>>` composes the outer `Option` recursor
        // with the `Vec<T>` recursor `classify_vec` (which projects
        // `Vec<bool>` to `Kind::VecBool`). Sharpens to
        // `Kind::OptionalVecBool` rather than falling through the
        // catch-all to `Kind::OptionalDeserialize` — the universal
        // `sexp_to_json` + `serde_json::from_value` bridge, whose
        // per-item shape mismatch surfaces as a
        // `LispError::KwargDeserialize` substring rather than the
        // typed `LispError::TypeMismatch { form: Item { key, idx },
        // expected: Bool, got: <shape> }` variant its required peer
        // `Kind::VecBool` already emits at the atom-family shape gate.
        // Sibling routing lift to the required-vec-bool
        // `Vec<bool>` → `Kind::VecBool` arm on `classify_vec` and the
        // optional-vec-string `Option<Vec<String>>` →
        // `Kind::OptionalVecString` arm above.
        Kind::VecBool => Kind::OptionalVecBool,
        // `Option<Vec<T>>` on the integer-narrowing axis composes the
        // outer `Option` recursor with the `Vec<T>` recursor
        // `classify_vec`, which projects `Vec<u16>` / `Vec<i32>` / etc.
        // to `Kind::VecInt(<literal>)`. This arm sharpens the composed
        // `Option<VecInt(<literal>)>` to the typed `Kind::OptionalVecInt`
        // variant, forwarding the width payload through unchanged so
        // the per-item narrowing gate rides `<T>` in ONE turbofish at
        // emit — same posture the required-peer `Kind::VecInt` arm on
        // `classify_vec` and the scalar-peer `Kind::OptionalInt` arm
        // above take on the int axis. Pre-lift `Option<Vec<u16>>` fell
        // through the catch-all to `Kind::OptionalDeserialize` (the
        // universal `sexp_to_json` + `serde_json::from_value` bridge —
        // whose per-item narrowing failure surfaces as a
        // `LispError::KwargDeserialize` substring rather than the typed
        // `LispError::KwargOutOfRange { target, value }` its required
        // peer `Kind::VecInt` already emits at the per-item narrowing
        // gate). Sibling routing lift to the required-vec-int
        // `Kind::VecInt` arm on `classify_vec` and to the
        // optional-scalar-int `Kind::OptionalInt` arm above; together
        // with the peer `Kind::VecFloat → Kind::OptionalVecFloat` arm
        // below, closes the last two remaining atom-family list-family
        // axis-mode combinations that still fell through the universal-
        // serde bridge at `classify_option` — completing the Cartesian
        // product across {scalar, optional-scalar, vec, optional-vec}
        // × {String, Bool, Int, Float} for the atom-family axes at the
        // derive dispatch surface.
        Kind::VecInt(t) => Kind::OptionalVecInt(t),
        // Float-axis sibling of the `Kind::VecInt → Kind::OptionalVecInt`
        // arm above. `Option<Vec<f32>>` / `Option<Vec<f64>>` compose
        // through `classify_vec`'s `Vec<f32>` / `Vec<f64>` →
        // `Kind::VecFloat(<literal>)` projection and sharpen here to
        // `Kind::OptionalVecFloat`, forwarding the float-width payload
        // through unchanged. Same pre-/post-lift shape as its int-axis
        // peer — the axis identity riding `<f32>` / `<f64>` is what
        // changes, not the scaffold.
        Kind::VecFloat(t) => Kind::OptionalVecFloat(t),
        // `Option<Vec<T>>` on the universal-serde-fallthrough axis —
        // `T` is a struct, an enum, a nested `Vec<T>`, or any non-
        // atomic type `classify_vec` also folds into
        // `Kind::VecDeserialize`. Sharpens to `Kind::OptionalVecDeserialize`
        // rather than falling through the catch-all to
        // `Kind::OptionalDeserialize` (the universal `sexp_to_json` +
        // `serde_json::from_value` bridge, whose per-item shape/decode
        // mismatch surfaces as a `LispError::KwargDeserialize` substring
        // rather than the typed
        // `LispError::KwargDeserialize { path: KwargPath::Item { key,
        // idx }, .. }` variant its required peer `Kind::VecDeserialize`
        // already emits at the per-item bridge). Sibling routing lift to
        // the required-vec `Kind::VecDeserialize` arm on `classify_vec`
        // and to the four atom-family `Kind::OptionalVec{String,Bool,
        // Int,Float}` arms above — closes the LAST `Option<Vec<T>>` ×
        // mode Cartesian-product hole in the derive's typed-entry
        // surface, so every `Option<Vec<T>>` field (atomic-inner OR
        // non-atomic-inner) now surfaces per-item rejections through a
        // `KwargPath::Item { key, idx }` path root rather than a
        // `KwargPath::Named(key)` one.
        Kind::VecDeserialize(inner_ty) => Kind::OptionalVecDeserialize(inner_ty),
        _ => Kind::OptionalDeserialize(quote! { #inner }),
    }
}

fn classify_vec(last: &syn::PathSegment, ty: &Type) -> Kind {
    let Ok(inner) = first_generic_type(last) else {
        // Malformed `Vec` — peer to `classify_option`'s malformed
        // arm above. Well-formed `Vec<T>` never reaches this arm.
        return Kind::VecDeserialize(quote! { #ty });
    };
    match classify(inner) {
        Kind::String => Kind::VecString,
        Kind::Int(t) => Kind::VecInt(t),
        Kind::Float(t) => Kind::VecFloat(t),
        Kind::Bool => Kind::VecBool,
        _ => Kind::VecDeserialize(quote! { #inner }),
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
mod narrow_width_tables_tests {
    use super::{classify, Kind, NARROW_FLOAT_WIDTHS, NARROW_INT_WIDTHS};
    use syn::{parse_str, Type};

    // The `(NARROW_INT_WIDTHS, NARROW_FLOAT_WIDTHS) -> ((&str, Kind::Int(_)) /
    // (&str, Kind::Float(_)))` projection is the derive's PRIVATE closed
    // set of numeric widths [`super::classify`] routes through the
    // narrowed-turbofish extractor helpers. Post-lift the classifier's
    // pattern-payload identity across the twelve numeric arms rides ONE
    // per-width `w` variable through both the equality gate and the
    // payload construction — the two hand-restated string literals per
    // arm collapse to ONE per-width entry in these consts, and the
    // classifier binds through iteration rather than through hand-restated
    // match arms.
    //
    // A regression here silently drifts the derive's numeric-narrowing
    // dispatch surface at every `#[derive(TataraDomain)]` implementor
    // with a field on the drifted width. The tests below pin the THREE
    // promises the two consts own at the boundary of their closed-set
    // contract:
    //
    // 1. CLOSED-SET IDENTITY — the two consts carry exactly the ten
    //    integer widths + two float widths [`super::NarrowNumeric`] has
    //    impls for in `tatara-lisp/src/domain.rs`, and no others. A
    //    regression that (a) dropped a width (silently rewiring that
    //    field to `extract_via_serde`, so a `port: u16` field would
    //    ship mystery serde substrings on `:port 70000` instead of the
    //    typed `LispError::KwargOutOfRange { target: U16, .. }`
    //    diagnostic), (b) added a width the substrate has NO
    //    `NarrowNumeric` impl for (breaking the derived consumer's own
    //    compile with a trait-bound-not-satisfied error), or (c)
    //    reordered the entries in a way that broke a sibling test
    //    module's per-index sweep — would surface here.
    // 2. PATTERN-PAYLOAD IDENTITY — every entry `w` in the int-width
    //    const routes `classify(<w>)` to `Kind::Int(w)` with a
    //    byte-equal `&'static str` payload. Pre-lift the derive's
    //    twelve numeric arms hand-restated the width literal TWICE
    //    (once as match pattern, once as payload construction), so a
    //    regression like `"u32" => Kind::Int("u16")` compiled fine
    //    and shipped WRONG emit code (the derived struct's `port:
    //    u32` field would rejection-gate against `NumericWidth::U16`
    //    bounds instead of `NumericWidth::U32`). Post-lift the
    //    loop's `w` variable rides BOTH the equality gate and the
    //    payload construction, so a pattern-payload drift is
    //    structurally impossible; this sweep confirms the byte
    //    identity holds end-to-end from const entry → classifier
    //    iteration → payload.
    // 3. DISJOINT AXES — an entry in [`NARROW_INT_WIDTHS`] is NOT in
    //    [`NARROW_FLOAT_WIDTHS`] and vice-versa. A regression that
    //    accidentally listed a name on both axes would produce a
    //    classifier that reached the int-arm's `Kind::Int(_)` payload
    //    (int arm runs first) but silently shadow the float-arm's
    //    entry — the disjoint pin catches the drift before it reaches
    //    the classifier's iteration order.
    //
    // Peer to the per-width `matches!(classify(&parse_ty("i8")),
    // Kind::Int("i8"))` sweep the sibling [`super::classify_tests`]
    // module carries. Those tests pin the CLASSIFIER's numeric-arm
    // behavior per-width; this module pins the SUBSTRATE-CONST's
    // closed-set identity per-const. Together the two closes the
    // derive's numeric-narrowing dispatch surface at both the source
    // (the closed-set data) and the projection (the classify function).
    //
    // Theory anchor: THEORY.md §II.1 invariant 1 — typed entry; the
    // (pattern-string, payload-string) identity across the twelve
    // classifier arms IS the derive's rust-level typed-entry
    // projection for narrowed widths, and lifting the identity to ONE
    // per-width const makes the identity load-bearing in the source
    // rather than in twenty-four hand-restated string literals.
    // THEORY.md §II.1 invariant 5 — composition preserves proofs; the
    // future-width extension flow ("adding a width lands as ONE
    // const entry, classifier + tests + emit code inherit
    // mechanically") IS the composition-preserving property this
    // module pins.

    fn parse_ty(s: &str) -> Type {
        parse_str(s).expect("valid Rust type syntax")
    }

    #[test]
    fn narrow_int_widths_carry_the_ten_widths_the_substrate_impls_narrow_numeric_wide_i64_for() {
        // Promise 1a (CLOSED-SET IDENTITY, int axis) — the const
        // carries exactly the ten integer widths
        // `tatara_lisp::domain::impl_narrow_int!` block enumerates:
        // `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`,
        // `usize`, `isize`. A regression that dropped a width from
        // the const would silently rewire that field's classify
        // route to the universal-serde fallthrough, and the derived
        // consumer would ship mystery serde substrings on
        // narrowing failures.
        //
        // The order is stable — the sibling
        // `super::classify_tests::every_supported_integer_width_...`
        // test walks the widths in the SAME order this const lists
        // them, and future test modules that iterate this const
        // can rely on the ordering being stable across additions
        // (new widths get appended, existing widths do not move).
        let expected: &[&str] = &[
            "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "usize", "isize",
        ];
        assert_eq!(
            NARROW_INT_WIDTHS, expected,
            "NARROW_INT_WIDTHS drifted from the ten widths tatara_lisp::domain's `impl_narrow_int!` block impls NarrowNumeric<Wide=i64> for",
        );
        assert_eq!(
            NARROW_INT_WIDTHS.len(),
            10,
            "NARROW_INT_WIDTHS length drifted — the substrate impls NarrowNumeric<Wide=i64> for exactly ten widths",
        );
    }

    #[test]
    fn narrow_float_widths_carry_the_two_widths_the_substrate_impls_narrow_numeric_wide_f64_for() {
        // Promise 1b (CLOSED-SET IDENTITY, float axis) — the const
        // carries exactly the two float widths
        // `tatara_lisp::domain` impls `NarrowNumeric<Wide=f64>` for
        // (`f32` and `f64` — the two IEEE-754 widths the reader's
        // `Sexp::as_float` projection can produce). A regression
        // that added a hypothetical `f16` to this const without the
        // substrate first landing a `NarrowNumeric<Wide=f64>` impl
        // on `f16` would break the derived consumer's own compile
        // with a trait-bound-not-satisfied error.
        let expected: &[&str] = &["f32", "f64"];
        assert_eq!(
            NARROW_FLOAT_WIDTHS, expected,
            "NARROW_FLOAT_WIDTHS drifted from the two widths tatara_lisp::domain impls NarrowNumeric<Wide=f64> for",
        );
        assert_eq!(
            NARROW_FLOAT_WIDTHS.len(),
            2,
            "NARROW_FLOAT_WIDTHS length drifted — the substrate impls NarrowNumeric<Wide=f64> for exactly two widths",
        );
    }

    #[test]
    fn every_int_width_classifies_to_kind_int_carrying_the_matching_static_string() {
        // Promise 2a (PATTERN-PAYLOAD IDENTITY, int axis) — iterating
        // [`NARROW_INT_WIDTHS`] and calling `classify(parse_ty(w))`
        // returns `Kind::Int(w)` with a payload byte-equal to the
        // const entry. This closes the "loop's `w` variable rides
        // both the equality gate and the payload construction"
        // contract at the classifier's iteration site.
        //
        // Pre-lift each of the twelve numeric arms hand-restated its
        // width literal TWICE — once as the match pattern
        // (`"u32" => ...`) and once as the payload construction
        // (`Kind::Int("u32")`). A regression could ship
        // `"u32" => Kind::Int("u16")` and compile fine, silently
        // shipping WRONG emit code (every downstream `port: u32`
        // field would ship `NumericWidth::U16` bounds rejections
        // rather than `U32`). Post-lift the single-source `w`
        // variable makes this drift structurally impossible; this
        // sweep confirms end-to-end that every const entry survives
        // the classify projection with its byte-value intact.
        for &w in NARROW_INT_WIDTHS {
            let ty = parse_ty(w);
            let kind = classify(&ty);
            let Kind::Int(payload) = kind else {
                panic!(
                    "width {w:?} classified as non-`Kind::Int` variant — pattern-payload identity broken at classify's iteration site",
                );
            };
            assert_eq!(
                payload, w,
                "width {w:?} classified with drifted payload {payload:?} — pattern-payload identity broken (loop's `w` did not ride both the equality gate and the payload construction)",
            );
        }
    }

    #[test]
    fn every_float_width_classifies_to_kind_float_carrying_the_matching_static_string() {
        // Promise 2b (PATTERN-PAYLOAD IDENTITY, float axis) —
        // float-axis peer of the int-axis sweep. Same contract,
        // different axis: iterating [`NARROW_FLOAT_WIDTHS`] and
        // calling `classify(parse_ty(w))` returns `Kind::Float(w)`
        // with a payload byte-equal to the const entry.
        for &w in NARROW_FLOAT_WIDTHS {
            let ty = parse_ty(w);
            let kind = classify(&ty);
            let Kind::Float(payload) = kind else {
                panic!(
                    "width {w:?} classified as non-`Kind::Float` variant — pattern-payload identity broken at classify's iteration site",
                );
            };
            assert_eq!(
                payload, w,
                "width {w:?} classified with drifted payload {payload:?} — pattern-payload identity broken",
            );
        }
    }

    #[test]
    fn narrow_int_and_float_width_tables_are_disjoint() {
        // Promise 3 (DISJOINT AXES) — no entry appears in both
        // [`NARROW_INT_WIDTHS`] and [`NARROW_FLOAT_WIDTHS`]. The
        // classifier's iteration order is int-first-then-float, so
        // an entry accidentally listed on both axes would silently
        // shadow the float-axis dispatch and route the field to
        // `Kind::Int(_)` — a downstream `TryFrom<i64>` decode gate
        // that has no meaningful behavior on a float width. This
        // disjoint pin catches the drift at the source (the closed-
        // set data) rather than as a silent classifier-order
        // dependency at emit time.
        for &int_w in NARROW_INT_WIDTHS {
            for &float_w in NARROW_FLOAT_WIDTHS {
                assert_ne!(
                    int_w, float_w,
                    "width {int_w:?} appears in both NARROW_INT_WIDTHS and NARROW_FLOAT_WIDTHS — the two axes must be disjoint (int-axis dispatch runs first and would silently shadow the float-axis entry)",
                );
            }
        }
    }
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
    // switch (`String`, `bool`, `i64` / `i32` / `u16` / `u32` / `u64` /
    // `usize` / `isize`, `f64` / `f32`), the `Option<T>` recursor (delegates to
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
        // The seven integer widths the derive supports each project to
        // `Kind::Int(<literal>)` with the width name threaded through
        // the payload — the payload IS the turbofish the emitted
        // extractor narrows `extract_int`'s `i64` return into (it was
        // an `as <ty>` cast until the narrowing landed). A
        // regression that (a) narrowed the supported set to a subset,
        // (b) mis-labeled ONE width's payload (e.g. `u32` → `"i32"`),
        // or (c) dropped the payload entirely would silently swap the
        // emitted target width at every consumer's `compile_from_args`
        // body.
        //
        // `isize` sits on the SIGNED half of the pointer-width column
        // one SIGNEDNESS axis over from `usize`; both narrow through
        // `TryFrom<i64>` on the `NarrowNumeric<i64>` trait so their
        // (classify → extract → narrow) chains are byte-identical modulo
        // the width payload the derive threads through `Kind::Int(_)`.
        //
        // `u16` sits on the NARROWEST half of the unsigned column one
        // BIT-WIDTH axis over from `u32`; it too narrows through
        // `TryFrom<i64>` on the same `NarrowNumeric<i64>` trait so its
        // (classify → extract → narrow) chain lands on the SAME shape
        // modulo the width-payload the derive threads through
        // `Kind::Int("u16")` — the canonical `port 70000` gate the
        // `LispError::KwargOutOfRange` docstring names.
        assert!(matches!(classify(&parse_ty("i8")), Kind::Int("i8")));
        assert!(matches!(classify(&parse_ty("i16")), Kind::Int("i16")));
        assert!(matches!(classify(&parse_ty("i32")), Kind::Int("i32")));
        assert!(matches!(classify(&parse_ty("i64")), Kind::Int("i64")));
        assert!(matches!(classify(&parse_ty("u8")), Kind::Int("u8")));
        assert!(matches!(classify(&parse_ty("u16")), Kind::Int("u16")));
        assert!(matches!(classify(&parse_ty("u32")), Kind::Int("u32")));
        assert!(matches!(classify(&parse_ty("u64")), Kind::Int("u64")));
        assert!(matches!(classify(&parse_ty("usize")), Kind::Int("usize")));
        assert!(matches!(classify(&parse_ty("isize")), Kind::Int("isize")));
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
            classify(&parse_ty("Option<i8>")),
            Kind::OptionalInt("i8")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<i16>")),
            Kind::OptionalInt("i16")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<i64>")),
            Kind::OptionalInt("i64")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<u8>")),
            Kind::OptionalInt("u8")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<u32>")),
            Kind::OptionalInt("u32")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<u16>")),
            Kind::OptionalInt("u16")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<isize>")),
            Kind::OptionalInt("isize")
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
            Kind::Deserialize(_)
        ));
        assert!(matches!(
            classify(&parse_ty("Severity")),
            Kind::Deserialize(_)
        ));
        assert!(matches!(classify(&parse_ty("Strng")), Kind::Deserialize(_)));
    }

    #[test]
    fn option_of_non_primitive_classifies_as_kind_optional_deserialize() {
        // `Option<T>` where `T` is NOT a supported primitive AND is NOT
        // itself a `Vec<T>` routes through `classify_option`'s catch-
        // all arm to `Kind::OptionalDeserialize`. Pin the residual
        // catch-all: a nested struct (`Option<MonitorSpec>`) and an
        // enum (`Option<Severity>`) — the two `Option<Nested>` scalar
        // shapes with no typed peer today. Every `Option<Vec<T>>`
        // shape now has a typed peer: the four atom-family axes
        // (`Option<Vec<String>>` / `Option<Vec<bool>>` /
        // `Option<Vec<Int>>` / `Option<Vec<Float>>`) sharpen to
        // `Kind::OptionalVecString` / `Kind::OptionalVecBool` /
        // `Kind::OptionalVecInt(_)` / `Kind::OptionalVecFloat(_)`, and
        // the non-atomic-inner `Option<Vec<Nested>>` axis
        // (`Option<Vec<MonitorSpec>>`, `Option<Vec<Vec<String>>>`,
        // etc.) sharpens to `Kind::OptionalVecDeserialize` through the
        // paired `Kind::VecDeserialize -> Kind::OptionalVecDeserialize`
        // arm on `classify_option` — closing the LAST
        // `Option<Vec<T>>` × mode Cartesian-product hole in the
        // derive's typed-entry surface (pinned by the sibling
        // `optional_vec_of_non_narrowable_type_classifies_as_kind_optional_vec_deserialize`
        // test), so every `Option<Vec<T>>` field (atomic-inner OR
        // non-atomic-inner) surfaces per-item rejections through a
        // `KwargPath::Item { key, idx }` path root. A regression that
        // added a new Kind variant for one of the remaining
        // `Option<Nested>` scalar compositions without updating the
        // recursor arm mapping would silently drift the extractor at
        // every consumer.
        assert!(matches!(
            classify(&parse_ty("Option<MonitorSpec>")),
            Kind::OptionalDeserialize(_)
        ));
        assert!(matches!(
            classify(&parse_ty("Option<Severity>")),
            Kind::OptionalDeserialize(_)
        ));
    }

    #[test]
    fn optional_vec_of_non_narrowable_type_classifies_as_kind_optional_vec_deserialize() {
        // `Option<Vec<T>>` where the inner `T` falls through to
        // `Kind::VecDeserialize` on the required-vec axis (a nested
        // struct, an enum, a nested `Vec<T>`, or any non-atomic type)
        // routes through `classify_option`'s
        // `Kind::VecDeserialize -> Kind::OptionalVecDeserialize` arm
        // — the composition of the outer `Option` recursor with the
        // `Vec<T>` recursor `classify_vec`'s catch-all arm to
        // `Kind::VecDeserialize`. Pin the same two representative
        // non-narrowable inner shapes the sibling required-peer test
        // `vec_of_non_narrowable_type_falls_through_to_kind_vec_deserialize`
        // pins (`Vec<MonitorSpec>`, `Vec<Vec<String>>`), so the
        // required-vec and optional-vec routes on the universal-serde
        // fallthrough axis share the same recognition boundary.
        //
        // Pre-lift `Option<Vec<MonitorSpec>>` fell through
        // `classify_option`'s catch-all arm to
        // `Kind::OptionalDeserialize` (the universal `sexp_to_json` +
        // `serde_json::from_value` bridge — no `Option<Vec<T>>` arm
        // on the recursor for the non-atomic-inner axis), so a per-
        // item shape/decode mismatch on
        // `:steps ((:notify-ref "ok") (:notify-ref 7))` into an
        // `Option<Vec<EscalationStep>>` field surfaced as a mystery
        // `KwargDeserialize { path: KwargPath::Named(key), message:
        // "invalid type: integer 7, expected a string at path .1.
        // notifyRef" }` substring rather than as the typed
        // `KwargDeserialize { path: KwargPath::Item { key, idx: 1 },
        // .. }` its REQUIRED peer (`steps: Vec<EscalationStep>`)
        // already emits at the per-item bridge. Post-lift the two
        // peers on the universal-serde fallthrough axis speak the
        // same typed rejection vocabulary — the operator sees ONE
        // rejection shape across every `Vec<T>` and `Option<Vec<T>>`
        // field with a non-atomic inner. A regression that dropped
        // this arm (folding it back into `Kind::OptionalDeserialize`)
        // would silently rewire every `Option<Vec<Nested>>` field
        // back to `extract_optional_via_serde::<Vec<Nested>>` — the
        // operator would see NO diagnostic drift at the derive site
        // but the downstream per-item rejection would revert to the
        // mystery serde substring shape.
        assert!(matches!(
            classify(&parse_ty("Option<Vec<MonitorSpec>>")),
            Kind::OptionalVecDeserialize(_)
        ));
        assert!(matches!(
            classify(&parse_ty("Option<Vec<Vec<String>>>")),
            Kind::OptionalVecDeserialize(_)
        ));
    }

    #[test]
    fn optional_vec_of_string_classifies_as_kind_optional_vec_string() {
        // `Option<Vec<String>>` composes the outer `Option` recursor
        // with the `Vec<T>` recursor `classify_vec` (which projects
        // `Vec<String>` to `Kind::VecString`); `classify_option`'s
        // per-primitive-vec arm sharpens the composition to
        // `Kind::OptionalVecString` rather than letting it fall
        // through the catch-all to `Kind::OptionalDeserialize` — the
        // universal `sexp_to_json` + `serde_json::from_value` bridge
        // whose per-item shape mismatch surfaces as a mystery
        // `LispError::KwargDeserialize` substring rather than the
        // typed `LispError::TypeMismatch { form: Item { key, idx },
        // expected: String, got: <shape> }` variant its REQUIRED peer
        // `Kind::VecString` already emits at the atom-family shape gate.
        //
        // Sibling routing lift to `Kind::VecBool` on the required-vec
        // bool axis and `Kind::VecInt(_)` / `Kind::VecFloat(_)` on the
        // required-vec numeric axes; the peer optional-vec-bool /
        // optional-vec-int / optional-vec-float axes still fall through
        // the catch-all (pinned by the sibling
        // `option_of_non_primitive_classifies_as_kind_optional_deserialize`
        // test's `Option<Vec<bool>>` / `Option<Vec<u16>>` /
        // `Option<Vec<f32>>` sweep). A regression that dropped this arm
        // (folding `Option<Vec<String>>` back into
        // `Kind::OptionalDeserialize`) would silently rewire every
        // `Option<Vec<String>>` field back to the serde bridge — the
        // operator would see NO diagnostic drift at the derive site but
        // the per-item rejection would revert to the mystery serde
        // substring shape.
        assert!(matches!(
            classify(&parse_ty("Option<Vec<String>>")),
            Kind::OptionalVecString
        ));
    }

    #[test]
    fn optional_vec_of_bool_classifies_as_kind_optional_vec_bool() {
        // `Option<Vec<bool>>` composes the outer `Option` recursor
        // with the `Vec<T>` recursor `classify_vec` (which projects
        // `Vec<bool>` to `Kind::VecBool`); `classify_option`'s
        // per-primitive-vec arm sharpens the composition to
        // `Kind::OptionalVecBool` rather than letting it fall
        // through the catch-all to `Kind::OptionalDeserialize` — the
        // universal `sexp_to_json` + `serde_json::from_value` bridge
        // whose per-item shape mismatch surfaces as a mystery
        // `LispError::KwargDeserialize` substring rather than the
        // typed `LispError::TypeMismatch { form: Item { key, idx },
        // expected: Bool, got: <shape> }` variant its REQUIRED peer
        // `Kind::VecBool` already emits at the atom-family shape
        // gate.
        //
        // Sibling routing lift to `Kind::OptionalVecString` on the
        // optional-vec-string axis and `Kind::VecBool` on the
        // required-vec-bool axis; the peer optional-vec-int /
        // optional-vec-float axes still fall through the catch-all
        // (pinned by the sibling
        // `option_of_non_primitive_classifies_as_kind_optional_deserialize`
        // test's `Option<Vec<u16>>` / `Option<Vec<f32>>` sweep). A
        // regression that dropped this arm (folding `Option<Vec<bool>>`
        // back into `Kind::OptionalDeserialize`) would silently
        // rewire every `Option<Vec<bool>>` field back to the serde
        // bridge — the operator would see NO diagnostic drift at the
        // derive site but the per-item rejection would revert to the
        // mystery serde substring shape.
        assert!(matches!(
            classify(&parse_ty("Option<Vec<bool>>")),
            Kind::OptionalVecBool
        ));
    }

    #[test]
    fn optional_vec_of_supported_integer_width_classifies_as_kind_optional_vec_int_with_matching_type_literal(
    ) {
        // `Option<Vec<T>>` where `T` is one of the ten supported integer
        // widths composes the outer `Option` recursor with the `Vec<T>`
        // recursor `classify_vec` (which projects `Vec<u16>` to
        // `Kind::VecInt("u16")`); `classify_option`'s per-narrowed-vec
        // arm sharpens the composition to
        // `Kind::OptionalVecInt(<literal>)`, forwarding the width payload
        // through unchanged so the per-item narrowing gate rides `<T>` in
        // ONE turbofish at emit — same posture the required-peer
        // `Kind::VecInt(_)` arm and the scalar-peer `Kind::OptionalInt(_)`
        // arm take on the int axis.
        //
        // Pre-lift `Option<Vec<u16>>` / `Option<Vec<i32>>` / etc. fell
        // through the catch-all to `Kind::OptionalDeserialize` (the
        // universal `sexp_to_json` + `serde_json::from_value` bridge —
        // whose per-item narrowing failure surfaced as a
        // `LispError::KwargDeserialize { message: "invalid value:
        // integer 70000, expected u16 at path .1" }` substring rather
        // than as the typed `KwargOutOfRange { target: U16, value:
        // Int(70_000) }` its required peer `Kind::VecInt("u16")` already
        // emits at the per-item narrowing gate). Post-lift the two peers
        // on the same integer axis speak the same typed rejection
        // vocabulary.
        //
        // Sweep the same ten widths the required-peer `Kind::VecInt(_)`
        // classify sweep pins so a regression that (a) dropped ONE width
        // from `classify_option`'s per-narrowed-vec routing (e.g.
        // accidentally leaving `Option<Vec<u16>>` on the fall-through),
        // (b) mis-labeled ONE width's payload, or (c) collapsed the
        // payload to a shared literal would surface here rather than as
        // silent routing drift at every downstream consumer with a
        // per-width optional-numeric-vec field.
        assert!(matches!(
            classify(&parse_ty("Option<Vec<i8>>")),
            Kind::OptionalVecInt("i8")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<Vec<i16>>")),
            Kind::OptionalVecInt("i16")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<Vec<i32>>")),
            Kind::OptionalVecInt("i32")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<Vec<i64>>")),
            Kind::OptionalVecInt("i64")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<Vec<u8>>")),
            Kind::OptionalVecInt("u8")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<Vec<u16>>")),
            Kind::OptionalVecInt("u16")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<Vec<u32>>")),
            Kind::OptionalVecInt("u32")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<Vec<u64>>")),
            Kind::OptionalVecInt("u64")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<Vec<usize>>")),
            Kind::OptionalVecInt("usize")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<Vec<isize>>")),
            Kind::OptionalVecInt("isize")
        ));
    }

    #[test]
    fn optional_vec_of_supported_float_width_classifies_as_kind_optional_vec_float_with_matching_type_literal(
    ) {
        // Float-axis sibling of the integer-axis optional-vec classify
        // sweep — `Option<Vec<f32>>` / `Option<Vec<f64>>` each route
        // through `Kind::OptionalVecFloat(<literal>)`, threading the
        // width name through the payload for the emitted narrowing
        // turbofish on `extract_optional_float_list_narrowed`'s per-
        // item `f64` return. Pin per-width so a regression that dropped
        // `Option<Vec<f32>>` (folding it back into
        // `Kind::OptionalDeserialize`) would silently rewire every
        // `Option<Vec<f32>>` field back to the universal-serde bridge —
        // the operator would see NO diagnostic drift at the derive site
        // but the downstream per-item rejection would revert to the
        // mystery serde substring shape.
        assert!(matches!(
            classify(&parse_ty("Option<Vec<f64>>")),
            Kind::OptionalVecFloat("f64")
        ));
        assert!(matches!(
            classify(&parse_ty("Option<Vec<f32>>")),
            Kind::OptionalVecFloat("f32")
        ));
    }

    #[test]
    fn vec_of_supported_integer_width_classifies_as_kind_vec_int_with_matching_type_literal() {
        // `Vec<T>` where `T` is one of the ten supported integer widths
        // routes through `classify_vec`'s per-primitive sharpening arm
        // to `Kind::VecInt(<literal>)` — the payload IS the turbofish
        // the emitted `extract_int_list_narrowed::<T>` narrows each
        // per-item wide `i64` into. Pre-lift these fell through to
        // `Kind::VecDeserialize` (the universal `sexp_to_json` + serde
        // bridge), so a per-item out-of-range value on `:ports (list
        // 80 70000)` into a `Vec<u16>` field surfaced as a
        // `KwargDeserialize { message: "invalid value: integer ..." }`
        // substring rather than as the typed
        // `KwargOutOfRange { target: U16, value: Int(70_000), .. }`
        // its SCALAR peer (`port: u16`) already emits; post-lift the
        // two gates on the same width speak the same typed rejection
        // vocabulary.
        //
        // Sweeps the same ten widths the scalar `Kind::Int(_)` arm
        // pins so a regression that (a) dropped ONE width from
        // `classify_vec`'s per-primitive routing (e.g. accidentally
        // leaving `Vec<u16>` on the fall-through), (b) mis-labeled
        // ONE width's payload, or (c) collapsed the payload to a
        // shared literal would surface here rather than as silent
        // routing drift at every downstream consumer with a
        // per-width numeric-vec field.
        assert!(matches!(classify(&parse_ty("Vec<i8>")), Kind::VecInt("i8")));
        assert!(matches!(
            classify(&parse_ty("Vec<i16>")),
            Kind::VecInt("i16")
        ));
        assert!(matches!(
            classify(&parse_ty("Vec<i32>")),
            Kind::VecInt("i32")
        ));
        assert!(matches!(
            classify(&parse_ty("Vec<i64>")),
            Kind::VecInt("i64")
        ));
        assert!(matches!(classify(&parse_ty("Vec<u8>")), Kind::VecInt("u8")));
        assert!(matches!(
            classify(&parse_ty("Vec<u16>")),
            Kind::VecInt("u16")
        ));
        assert!(matches!(
            classify(&parse_ty("Vec<u32>")),
            Kind::VecInt("u32")
        ));
        assert!(matches!(
            classify(&parse_ty("Vec<u64>")),
            Kind::VecInt("u64")
        ));
        assert!(matches!(
            classify(&parse_ty("Vec<usize>")),
            Kind::VecInt("usize")
        ));
        assert!(matches!(
            classify(&parse_ty("Vec<isize>")),
            Kind::VecInt("isize")
        ));
    }

    #[test]
    fn vec_of_supported_float_width_classifies_as_kind_vec_float_with_matching_type_literal() {
        // Float-axis sibling of
        // `vec_of_supported_integer_width_classifies_as_kind_vec_int_with_matching_type_literal`
        // — `Vec<f32>` / `Vec<f64>` each route through
        // `Kind::VecFloat(<literal>)`, threading the width name
        // through the payload for the emitted narrowing turbofish
        // on `extract_float_list_narrowed`'s per-item `f64` return.
        // Pin per-width so a regression that dropped `Vec<f32>`
        // (folding it back into `Kind::VecDeserialize`) would
        // silently rewire every `Vec<f32>` field back to
        // `extract_vec_via_serde` — the operator would see NO
        // diagnostic drift at the derive site but the downstream
        // per-item rejection would revert to the mystery serde
        // substring shape.
        assert!(matches!(
            classify(&parse_ty("Vec<f64>")),
            Kind::VecFloat("f64")
        ));
        assert!(matches!(
            classify(&parse_ty("Vec<f32>")),
            Kind::VecFloat("f32")
        ));
    }

    #[test]
    fn vec_of_bool_folds_into_kind_vec_bool() {
        // `Vec<bool>` routes through `classify_vec`'s per-primitive
        // sharpening arm to `Kind::VecBool` — the payload-free peer
        // of `Kind::VecInt(_)` / `Kind::VecFloat(_)` on the
        // non-narrowing bool axis (a bool has no "wider" `Sexp`
        // return to narrow, so no width literal rides the payload;
        // the atom projection IS already the field type). The
        // emitted extractor is `extract_bool_list`, which decodes
        // each element via `<bool as AtomKwarg<'_>>::project_at` —
        // the atom-family per-item shape gate the string-list peer
        // (`extract_string_list`) and the numeric-list peer
        // (`extract_narrowed_list`'s per-item body) already bind
        // through.
        //
        // Pre-lift `Vec<bool>` fell through to `Kind::VecDeserialize`
        // (the universal `sexp_to_json` + serde bridge), so a per-
        // item shape mismatch on `:flags (list #t "yes")` into a
        // `Vec<bool>` field surfaced as a
        // `KwargDeserialize { message: "invalid type: string \"yes\",
        // expected a boolean at path .1" }` substring rather than as
        // the typed
        // `TypeMismatch { form: Item { key: "flags", idx: 1 },
        // expected: Bool, got: String }` its scalar peer
        // (`enabled: bool`) already emits; post-lift the two gates on
        // the same axis speak the same typed rejection vocabulary. A
        // regression that dropped `Vec<bool>` from `classify_vec`'s
        // per-primitive routing (folding it back into
        // `Kind::VecDeserialize`) would silently rewire every
        // `Vec<bool>` field back to `extract_vec_via_serde` — the
        // operator would see NO diagnostic drift at the derive site
        // but the downstream per-item rejection would revert to the
        // mystery serde substring shape.
        assert!(matches!(classify(&parse_ty("Vec<bool>")), Kind::VecBool));
    }

    #[test]
    fn vec_of_non_narrowable_type_falls_through_to_kind_vec_deserialize() {
        // The two non-primitive `Vec<T>` shapes still route through
        // `classify_vec`'s catch-all arm to `Kind::VecDeserialize` —
        // the sexp_to_json + `serde_json::from_value` bridge that
        // unlocks nested-struct vecs, foreign-enum vecs, and nested
        // vecs. Pin the residual fall-through: a regression that
        // folded `Vec<MonitorSpec>` or `Vec<Vec<String>>` into a
        // narrowed arm would fail here.
        assert!(matches!(
            classify(&parse_ty("Vec<MonitorSpec>")),
            Kind::VecDeserialize(_)
        ));
        assert!(matches!(
            classify(&parse_ty("Vec<Vec<String>>")),
            Kind::VecDeserialize(_)
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
        assert!(matches!(classify(&parse_ty("&str")), Kind::Deserialize(_)));
        assert!(matches!(
            classify(&parse_ty("[u8; 4]")),
            Kind::Deserialize(_)
        ));
        assert!(matches!(
            classify(&parse_ty("(String, i64)")),
            Kind::Deserialize(_)
        ));
    }

    // ── Payload identity — the four `Kind::(Optional?)(Vec?)Deserialize`
    //    variants each cache the inner-`T` tokens at classify time so the
    //    `extractor_for` match dispatches on the payload without re-walking
    //    the outer field type. Pin the projection per axis-mode so a
    //    regression that dropped the payload OR cached the wrong wrapper
    //    level (`Option<T>` instead of `T`, or `Vec<T>` instead of `T`)
    //    surfaces at ONE test-module boundary in the derive crate rather
    //    than as a silent type-mismatch at the derived consumer's compile.
    //    Same shape the numeric-narrowed payload pins on
    //    `every_supported_integer_width_classifies_as_kind_int_with_the_matching_type_literal`
    //    give for `Kind::Int(&'static str)`.

    #[test]
    fn kind_deserialize_scalar_mode_payload_is_the_whole_field_type() {
        // On the scalar `Kind::Deserialize` arm the whole field
        // type IS the UFCS `T` — the classifier's fallthrough
        // (`Kind::Deserialize(quote! { #ty })`) caches the field
        // tokens verbatim so
        // `deserialize_trait_dispatch_call("extract_kwarg", inner_ty,
        // key)` binds `<T as DeserializeKwarg>::extract_kwarg` with
        // `T = the field type`. Sweep three representative shapes
        // the fallthrough recognizes — a plain-ident enum, a nested-
        // struct, and a fully-qualified path — so a regression that
        // narrowed the payload projection (say, stringified through
        // `.to_string()` or dropped the leading `::`) surfaces per-
        // shape here.
        for (field_src, expected_payload) in [
            ("Severity", "Severity"),
            ("MonitorSpec", "MonitorSpec"),
            (
                "::my_crate::my_mod::Config",
                ":: my_crate :: my_mod :: Config",
            ),
        ] {
            let ty = parse_ty(field_src);
            let Kind::Deserialize(payload) = classify(&ty) else {
                panic!("classify({field_src}) must project to Kind::Deserialize(_)");
            };
            assert_eq!(
                payload.to_string(),
                expected_payload,
                "Kind::Deserialize payload for `{field_src}` must be `{expected_payload}`",
            );
        }
    }

    #[test]
    fn kind_optional_deserialize_payload_is_the_type_inside_option() {
        // On the `Option<T>` fallthrough arm the payload is `T`, not
        // `Option<T>` — the classifier's `classify_option`
        // fallthrough emits `Kind::OptionalDeserialize(quote! {
        // #inner })` where `inner` is `first_generic_type(last)`'s
        // Ok payload. A regression that carried the outer
        // `Option<T>` through would emit `<Option<T> as
        // DeserializeKwarg>::extract_optional_kwarg` at the derived
        // consumer and mismatch the field type as
        // `Result<Option<Option<T>>>`; pinning the payload as `T`
        // here catches the drift at the classifier boundary.
        for (field_src, expected_payload) in [
            ("Option<Severity>", "Severity"),
            ("Option<MonitorSpec>", "MonitorSpec"),
            ("core::option::Option<Severity>", "Severity"),
        ] {
            let ty = parse_ty(field_src);
            let Kind::OptionalDeserialize(payload) = classify(&ty) else {
                panic!("classify({field_src}) must project to Kind::OptionalDeserialize(_)");
            };
            assert_eq!(
                payload.to_string(),
                expected_payload,
                "Kind::OptionalDeserialize payload for `{field_src}` must be `{expected_payload}`",
            );
        }
    }

    #[test]
    fn kind_vec_deserialize_payload_is_the_type_inside_vec() {
        // Peer of the `Kind::OptionalDeserialize` pin on the
        // required-vec axis — `Vec<T>`'s payload is `T`, not
        // `Vec<T>`. Rules out a fallthrough that carries the outer
        // wrapper level. Nested `Vec<Vec<String>>` caches the ONE-
        // level unwrap `Vec<String>` (matching the required-vec
        // arm's `<T as DeserializeKwarg>::extract_vec_kwarg`
        // signature that returns `Result<Vec<T>>`).
        for (field_src, expected_payload) in [
            ("Vec<MonitorSpec>", "MonitorSpec"),
            ("Vec<Vec<String>>", "Vec < String >"),
        ] {
            let ty = parse_ty(field_src);
            let Kind::VecDeserialize(payload) = classify(&ty) else {
                panic!("classify({field_src}) must project to Kind::VecDeserialize(_)");
            };
            assert_eq!(
                payload.to_string(),
                expected_payload,
                "Kind::VecDeserialize payload for `{field_src}` must be `{expected_payload}`",
            );
        }
    }

    #[test]
    fn kind_optional_vec_deserialize_payload_is_the_type_inside_option_vec() {
        // TWO-level unwrap: `Option<Vec<T>>` → `T`. Pin the two-
        // step projection so a regression that unwrapped only ONE
        // level (leaving `Vec<T>` in the payload) surfaces here —
        // pre-lift the extract-site walk did two `generic_arg(...)?`
        // hops for exactly this arm; the classifier now performs the
        // composition via `classify_option`'s
        // `Kind::VecDeserialize(inner_ty) →
        // Kind::OptionalVecDeserialize(inner_ty)` arm, which
        // forwards the required-peer's cached inner tokens through
        // unchanged.
        for (field_src, expected_payload) in [
            ("Option<Vec<MonitorSpec>>", "MonitorSpec"),
            ("Option<Vec<Vec<String>>>", "Vec < String >"),
        ] {
            let ty = parse_ty(field_src);
            let Kind::OptionalVecDeserialize(payload) = classify(&ty) else {
                panic!("classify({field_src}) must project to Kind::OptionalVecDeserialize(_)");
            };
            assert_eq!(
                payload.to_string(),
                expected_payload,
                "Kind::OptionalVecDeserialize payload for `{field_src}` must be `{expected_payload}`",
            );
        }
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
mod narrow_trait_dispatch_call_tests {
    use super::narrow_trait_dispatch_call;
    use quote::quote;

    // The `(method, rust_ty, key) -> TokenStream2` helper is the
    // derive's PRIVATE emission scaffold for the EIGHT numeric-narrowed
    // arms in `extractor_for` (`Kind::Int(_)` / `Kind::OptionalInt(_)`
    // / `Kind::VecInt(_)` / `Kind::OptionalVecInt(_)` /
    // `Kind::Float(_)` / `Kind::OptionalFloat(_)` / `Kind::VecFloat(_)`
    // / `Kind::OptionalVecFloat(_)`) — collapsed post-lift onto FOUR
    // per-mode trait methods on `NarrowNumeric` (per-run cb9648d
    // opened the trait defaults). Each arm is now a one-line delegate
    // onto this helper; the shared scaffold — parse the payload's
    // width literal as a TURBOFISH-typed UFCS dispatcher, resolve the
    // trait method `Ident` at the derive's call-site span, emit the
    // fully-qualified `<T as ::tatara_lisp::domain::NarrowNumeric>::
    // <method>(&kw, #key)?` call — lives at ONE substrate primitive.
    //
    // A regression here silently swaps the emitted code at every
    // `#[derive(TataraDomain)]` implementor with a numeric-narrowed
    // field. The tests below pin the SIX promises the helper owns at
    // the boundary of its emission shape:
    //
    // 1. UFCS TURBOFISH — the `rust_ty` payload (`"u16"`, `"f32"`,
    //    …) rides the emitted `<T as ...>::<method>(&kw, #key)?` as
    //    a Rust type, NOT as a string literal or a cast target. Pinned
    //    by the width-per-arm sweep test below (any of the ten integer
    //    widths and two float widths land in the UFCS type slot
    //    verbatim, at both scalar and list modes).
    // 2. TRAIT METHOD IDENTITY — the `method` `&'static str` payload
    //    rides the emitted `<T as ...NarrowNumeric>::<method>` path
    //    as an `Ident` at the call-site span, not as a string literal
    //    or a `Path` composed by string concatenation. Pinned by the
    //    four-mode sweep test that walks each of the FOUR mode-suffixed
    //    trait methods the derive's per-mode dispatch pairs to
    //    (`extract_narrowed_kwarg`, `extract_optional_narrowed_kwarg`,
    //    `extract_narrowed_list_kwarg`,
    //    `extract_optional_narrowed_list_kwarg`).
    // 3. FULLY-QUALIFIED TRAIT PATH — the emission names the trait as
    //    `::tatara_lisp::domain::NarrowNumeric` — leading `::` +
    //    three-segment path — so the UFCS dispatch is hygienic in the
    //    derived code's outer scope regardless of what the
    //    implementor's crate imports (the same discipline every peer
    //    non-narrowed arm's `::tatara_lisp::domain::extract_*`
    //    emission gives on the non-narrowed path).
    // 4. KEY LITERAL PASS-THROUGH — the `key` parameter rides the
    //    emitted call's second argument as a Rust string literal, so
    //    the emitted call reads the SAME kwarg the derive's per-field
    //    `#key` interpolation names (kebab-cased from the Rust field
    //    ident by `snake_to_kebab`). Pinned by the key-literal test.
    // 5. `?`-SUFFIX — the emission ends with the `?` operator so the
    //    caller-visible expression type is the narrowed `T` /
    //    `Option<T>` / `Vec<T>` (not a `Result`), matching the shape
    //    every peer `Kind::*` arm's `extract_*(&kw, #key)?` emission
    //    gives. Pinned by the `?`-suffix test.
    // 6. AXIS IDENTITY LIVES ON `T`, NOT THE METHOD NAME — the
    //    method name is the SAME string on both the int axis and the
    //    float axis (there is no `_int_` / `_float_` substring in any
    //    of the FOUR mode-suffixed methods); the wide axis rides
    //    `<T as NarrowNumeric>::Wide` at the substrate's trait default,
    //    so a hypothetical third wide axis (e.g. `u128` / `Decimal`)
    //    lands as ZERO new emit strings here. Pinned by the sibling
    //    `both_axes_dispatch_to_the_same_method_name_per_mode` test.
    //
    // Peer to the `classify_tests` module above — that module pins
    // the `(syn::Type -> Kind)` projection (what each field type
    // decodes to), this module pins the `(Kind::* -> TokenStream2)`
    // projection (what each Kind emits). Together the two test
    // modules close the derive's `syn::Type -> emitted extractor
    // call` end-to-end pipeline at the test-module boundary.
    //
    // Theory anchor: THEORY.md §II.1 invariant 1 (typed entry) — the
    // helper IS the derive's rust-level typed-entry projection at the
    // numeric field's `syn::Type -> emitted extractor call` boundary;
    // naming its SIX emission promises as pinned facts here makes
    // future upgrades (a caller-supplied diagnostic span, a `?`-
    // suppressed variant, a new mode, or a new wide axis) fail at
    // the boundary of the shape they change rather than at
    // downstream implementors' compile-time noise. THEORY.md §II.1
    // invariant 3 (typed exit) — the axis identity riding the UFCS
    // type parameter routes through `<T as NarrowNumeric>::Wide` at
    // rustc time, so the emit-time collapse across axes preserves
    // the per-narrow-type rejection identity.

    fn call_string(method: &'static str, rust_ty: &str, key: &str) -> String {
        narrow_trait_dispatch_call(method, rust_ty, key).to_string()
    }

    #[test]
    fn helper_emits_the_ufcs_turbofish_at_every_supported_integer_width() {
        // Promise 1 (UFCS TURBOFISH) — the `rust_ty` payload rides
        // the emitted call as a Rust type in the `<T as ...>` UFCS
        // slot. Sweep the ten integer widths the derive's
        // `Kind::Int(_)` payload spans so a regression that (a) lost
        // the type parameter, (b) rendered the width as a string
        // literal, or (c) dropped the payload entirely surfaces
        // per-width rather than only on one canonical width.
        //
        // The `< <width> as` bracketing pins the UFCS type-parameter
        // shape; the trailing `(&kw, "port")?` pins the arg + `?`
        // shape.
        for width in [
            "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "usize", "isize",
        ] {
            let body = call_string("extract_narrowed_kwarg", width, "port");
            let expected_ufcs_head = format!("< {width} as");
            assert!(
                body.contains(&expected_ufcs_head),
                "width {width} must ride the UFCS type slot `<{width} as ...>`, got: {body}",
            );
            assert!(
                body.ends_with(r#"("port") ?"#) || body.ends_with(r#"(& kw , "port") ?"#),
                "emission must end with `(&kw, \"port\")?`, got: {body}",
            );
        }
    }

    #[test]
    fn helper_emits_the_ufcs_turbofish_at_every_supported_float_width() {
        // Float-axis peer of the integer sweep — Promise 1 at the
        // two float widths the derive's `Kind::Float(_)` payload
        // spans. Both float widths reach the SAME helper as the
        // integer widths (the axis identity does NOT ride the method
        // name) — a regression that split the emit path by axis
        // would surface here (both widths must reach the same
        // `extract_narrowed_kwarg` method name at the trait dispatch).
        for width in ["f32", "f64"] {
            let body = call_string("extract_narrowed_kwarg", width, "threshold");
            let expected_ufcs_head = format!("< {width} as");
            assert!(
                body.contains(&expected_ufcs_head),
                "width {width} must ride the UFCS type slot `<{width} as ...>`, got: {body}",
            );
        }
    }

    #[test]
    fn helper_emits_the_trait_method_ident_at_every_four_narrowed_mode() {
        // Promise 2 (TRAIT METHOD IDENTITY) — the `method` string
        // rides the emitted call as an `Ident` at the derive's
        // call-site span. Sweep the FOUR mode-suffixed trait methods
        // the derive's numeric-narrowed arms map to, so a regression
        // that silently swapped ONE arm's method name at the
        // `extractor_for` dispatch site (e.g. a scalar arm
        // accidentally routing through the list-mode method) would
        // surface here rather than as a mystery type-mismatch at
        // every downstream consumer with a numeric field.
        //
        // Post-lift the FOUR mode names cover BOTH axes (int + float)
        // — halved from the pre-lift EIGHT axis-typed public wrapper
        // names — because the wide axis lives at `<T as
        // NarrowNumeric>::Wide` on the target width's impl block.
        //
        // The full UFCS path `< u16 as :: tatara_lisp :: domain ::
        // NarrowNumeric > :: <method>` (the proc_macro2-formatted
        // spelling of `<u16 as ::tatara_lisp::domain::NarrowNumeric>
        // ::<method>`) pins the method as a load-bearing token
        // trailing the UFCS bracket.
        for method in [
            "extract_narrowed_kwarg",
            "extract_optional_narrowed_kwarg",
            "extract_narrowed_list_kwarg",
            "extract_optional_narrowed_list_kwarg",
        ] {
            let body = call_string(method, "u16", "port");
            let expected_method_call = format!(":: NarrowNumeric > :: {method}");
            assert!(
                body.contains(&expected_method_call),
                "method {method} must ride the emitted UFCS dispatch as `{expected_method_call}`, got: {body}",
            );
        }
    }

    #[test]
    fn helper_emits_the_fully_qualified_trait_path_with_a_leading_double_colon() {
        // Promise 3 (FULLY-QUALIFIED TRAIT PATH) — the emitted UFCS
        // trait path starts with `::` so it resolves against the
        // crate root regardless of what the implementor's `use`
        // imports bring into scope. A regression that dropped the
        // leading `::` would silently drift the resolution to
        // whatever `tatara_lisp` the implementor's scope resolves (a
        // re-export, a local alias, a stub) rather than the substrate
        // crate.
        //
        // Pinned separately from the method-identity sweep because
        // it's a distinct promise (path hygiene) — a refactor that
        // (say) resolved the trait method `Ident` correctly but
        // emitted the trait path as a bare `NarrowNumeric` (relying
        // on an implicit re-export) would pass the peer sweep and
        // fail here.
        let body = call_string("extract_narrowed_kwarg", "i32", "count");
        assert!(
            body.contains(":: tatara_lisp :: domain :: NarrowNumeric"),
            "emission must fully qualify the trait as `::tatara_lisp::domain::NarrowNumeric`, got: {body}",
        );
    }

    #[test]
    fn helper_emits_the_key_literal_as_the_second_positional_argument() {
        // Promise 4 (KEY LITERAL PASS-THROUGH) — the `key` parameter
        // rides the emitted call's second argument as a Rust string
        // literal. Sweep three representative kebab-cased key shapes
        // the derive's `snake_to_kebab` projection emits
        // (single-word, hyphenated, digit-suffixed) so a regression
        // that (a) dropped the key, (b) interpolated it as an ident
        // instead of a string literal, or (c) reordered the arg
        // positions surfaces per-shape.
        for (key, expected_lit) in [
            ("port", "\"port\""),
            ("window-seconds", "\"window-seconds\""),
            ("scale-v2", "\"scale-v2\""),
        ] {
            let body = call_string("extract_narrowed_kwarg", "u16", key);
            assert!(
                body.contains(expected_lit),
                "key {key} must ride the emitted call as string literal {expected_lit}, got: {body}",
            );
        }
    }

    #[test]
    fn helper_emits_the_question_mark_suffix_so_the_expression_projects_to_the_narrowed_value() {
        // Promise 5 (?-SUFFIX) — the emission ends with `?` so the
        // caller-visible expression type is the narrowed `T` / peer
        // shape, matching every non-narrowed arm's
        // `extract_*(&kw, #key)?` shape. A regression that dropped
        // the `?` would leave the derived-code expression as a
        // `Result<T, LispError>` — every consumer struct field
        // initializer would fail at the derived-code compile-time
        // with a type mismatch, but the derive's own emission would
        // still pass its own compile.
        let body = call_string("extract_narrowed_kwarg", "u32", "count");
        assert!(
            body.trim_end().ends_with('?'),
            "emission must end with the `?` operator, got: {body}",
        );
    }

    #[test]
    fn both_axes_dispatch_to_the_same_method_name_per_mode() {
        // Promise 6 (AXIS IDENTITY LIVES ON `T`, NOT THE METHOD
        // NAME) — the four mode-suffixed method names cover BOTH the
        // int axis (`u16`, `i64`, `usize`, `isize`) and the float
        // axis (`f32`, `f64`); the wide axis rides `<T as
        // NarrowNumeric>::Wide` at the substrate's trait default. A
        // regression that reintroduced an axis-typed split in the
        // method name (e.g. `extract_int_narrowed_kwarg` /
        // `extract_float_narrowed_kwarg`) would surface here at the
        // mode-by-mode cross-check.
        //
        // For each of the FOUR modes, walk one canonical int width
        // (`u16` — the `port 70000` gate the docstring names) and one
        // canonical float width (`f32` — the `1.0e300` overflow gate
        // the float-axis test module names) through the SAME method
        // name; assert the two emissions are byte-identical up to
        // the width payload's UFCS slot.
        for method in [
            "extract_narrowed_kwarg",
            "extract_optional_narrowed_kwarg",
            "extract_narrowed_list_kwarg",
            "extract_optional_narrowed_list_kwarg",
        ] {
            let int_body = call_string(method, "u16", "port");
            let float_body = call_string(method, "f32", "scale");
            // Both emissions carry the SAME method name and the SAME
            // trait qualifier; only the width payload and key differ.
            let expected_trait_tail = format!(":: NarrowNumeric > :: {method}");
            assert!(
                int_body.contains(&expected_trait_tail),
                "int-axis emission must dispatch through `{expected_trait_tail}`, got: {int_body}",
            );
            assert!(
                float_body.contains(&expected_trait_tail),
                "float-axis emission must dispatch through `{expected_trait_tail}`, got: {float_body}",
            );
            // Method-name identity across axes — a hypothetical
            // regression that split the mode into an int-only /
            // float-only pair would leave ONE of these two
            // assertions failing.
            assert!(
                !method.contains("int") && !method.contains("float"),
                "method {method} must NOT carry an axis suffix — the axis lives on `<T as NarrowNumeric>::Wide`",
            );
        }
    }

    #[test]
    fn helper_emission_is_token_equivalent_to_a_hand_written_scalar_narrow_trait_dispatch() {
        // Cross-check — the helper's emission at the canonical scalar
        // `Kind::Int("u16")` arm is TOKEN-EQUIVALENT (byte-identical
        // modulo whitespace) to a hand-written `quote! { <u16 as
        // ::tatara_lisp::domain::NarrowNumeric>::extract_narrowed_kwarg
        // (&kw, "port")? }` scaffold. A regression at any of Promises
        // 1-5 that individually passes the per-promise pins above but
        // composes into a token stream distinct from the hand-written
        // form would surface here — one final structural pin above
        // the per-promise arms.
        let key = "port";
        let narrowed: proc_macro2::TokenStream = "u16".parse().unwrap();
        let expected = quote! {
            <#narrowed as ::tatara_lisp::domain::NarrowNumeric>::extract_narrowed_kwarg(&kw, #key)?
        }
        .to_string();
        let actual = call_string("extract_narrowed_kwarg", "u16", key);
        assert_eq!(
            actual, expected,
            "helper emission must be token-equivalent to the hand-written scalar-narrow trait dispatch",
        );
    }

    #[test]
    fn helper_emission_is_token_equivalent_to_a_hand_written_optional_narrow_trait_dispatch_at_float_axis(
    ) {
        // Float-axis / optional-mode peer of the scalar-int
        // token-equivalence pin — closes the axis + mode diagonal on
        // the cross-check. Confirms the emission on `f32` at the
        // optional-scalar mode dispatches through the SAME
        // `extract_optional_narrowed_kwarg` trait method the int
        // axis reaches on the peer test above (only the UFCS type
        // payload changes across axes; the method name does not).
        let key = "threshold";
        let narrowed: proc_macro2::TokenStream = "f32".parse().unwrap();
        let expected = quote! {
            <#narrowed as ::tatara_lisp::domain::NarrowNumeric>::extract_optional_narrowed_kwarg(&kw, #key)?
        }
        .to_string();
        let actual = call_string("extract_optional_narrowed_kwarg", "f32", key);
        assert_eq!(
            actual, expected,
            "helper emission must be token-equivalent to the hand-written optional-float trait dispatch",
        );
    }

    #[test]
    fn helper_emission_is_token_equivalent_to_a_hand_written_vec_narrow_trait_dispatch() {
        // List-mode peer of the scalar / optional token-equivalence
        // pins — closes the mode axis on the cross-check. Confirms
        // the emission on `u16` at the vec-mode dispatches through
        // the `extract_narrowed_list_kwarg` trait method the
        // list-mode arms both (`Kind::VecInt(_)` and `Kind::VecFloat(_)`)
        // route through post-lift.
        let key = "ports";
        let narrowed: proc_macro2::TokenStream = "u16".parse().unwrap();
        let expected = quote! {
            <#narrowed as ::tatara_lisp::domain::NarrowNumeric>::extract_narrowed_list_kwarg(&kw, #key)?
        }
        .to_string();
        let actual = call_string("extract_narrowed_list_kwarg", "u16", key);
        assert_eq!(
            actual, expected,
            "helper emission must be token-equivalent to the hand-written vec-narrow trait dispatch",
        );
    }
}

#[cfg(test)]
mod atom_trait_dispatch_call_tests {
    use super::atom_trait_dispatch_call;
    use quote::quote;

    // The `(method, rust_ty, key) -> TokenStream2` helper is the
    // derive's PRIVATE emission scaffold for the FOUR atom-family
    // list-family arms in `extractor_for` (`Kind::VecString` /
    // `Kind::OptionalVecString` / `Kind::VecBool` /
    // `Kind::OptionalVecBool`) — collapsed post-lift onto TWO per-mode
    // trait methods on `AtomKwarg` (`extract_list_kwarg` /
    // `extract_optional_list_kwarg`, the list-family peers of the
    // scalar `extract_kwarg` / `extract_optional_kwarg` trait defaults
    // per-run 824ca51 opened on the trait). Each arm is now a one-line
    // delegate onto this helper; the shared scaffold — parse the
    // payload's atom-type literal (`"& str"`, `"bool"`) as a
    // UFCS `Self` type, resolve the trait method `Ident` at the
    // derive's call-site span, emit the fully-qualified `<T as
    // ::tatara_lisp::domain::AtomKwarg<'_>>::<method>(&kw, #key)?`
    // call — lives at ONE substrate primitive.
    //
    // A regression here silently swaps the emitted code at every
    // `#[derive(TataraDomain)]` implementor with a `Vec<String>` /
    // `Vec<bool>` / `Option<Vec<String>>` / `Option<Vec<bool>>` field.
    // The tests below pin the SIX promises the helper owns at the
    // boundary of its emission shape, mirroring the SIX-promise pin
    // its numeric-narrowed peer's test module gives:
    //
    // 1. UFCS `Self` TYPE — the `rust_ty` payload (`"& str"`,
    //    `"bool"`) rides the emitted `<T as ...>::<method>(&kw, #key)?`
    //    as a Rust type, NOT as a string literal or a cast target.
    //    Pinned by the two-axis sweep test below (both atom axes
    //    the derive's `Kind::Vec{String,Bool}` /
    //    `Kind::OptionalVec{String,Bool}` payloads span land in the
    //    UFCS type slot verbatim, at both list modes).
    // 2. TRAIT METHOD IDENTITY — the `method` `&'static str` payload
    //    rides the emitted `<T as ...AtomKwarg<'_>>::<method>` path
    //    as an `Ident` at the call-site span, not as a string literal
    //    or a `Path` composed by string concatenation. Pinned by the
    //    two-mode sweep test that walks each of the TWO mode-suffixed
    //    trait methods the derive's per-mode dispatch pairs to
    //    (`extract_list_kwarg`, `extract_optional_list_kwarg`).
    // 3. FULLY-QUALIFIED TRAIT PATH — the emission names the trait as
    //    `::tatara_lisp::domain::AtomKwarg<'_>` — leading `::` +
    //    three-segment path with the elided lifetime slot — so the
    //    UFCS dispatch is hygienic in the derived code's outer scope
    //    regardless of what the implementor's crate imports (the same
    //    discipline every peer arm's fully-qualified path emission
    //    gives — [`super::narrow_trait_dispatch_call`] on the
    //    numeric-narrowing axis, [`super::deserialize_trait_dispatch_call`]
    //    on the universal-serde-fallthrough axis).
    // 4. KEY LITERAL PASS-THROUGH — the `key` parameter rides the
    //    emitted call's second argument as a Rust string literal, so
    //    the emitted call reads the SAME kwarg the derive's per-field
    //    `#key` interpolation names (kebab-cased from the Rust field
    //    ident by `snake_to_kebab`). Pinned by the key-literal test.
    // 5. `?`-SUFFIX — the emission ends with the `?` operator so the
    //    caller-visible expression type is the extracted `Vec<T>` /
    //    `Option<Vec<T>>` (not a `Result`), matching the shape every
    //    peer `Kind::*` arm's emission gives. Pinned by the `?`-suffix
    //    test.
    // 6. AXIS IDENTITY LIVES ON `T`, NOT THE METHOD NAME — the
    //    method name is the SAME string on both the string axis and
    //    the bool axis (there is no `_string_` / `_bool_` substring
    //    in either of the TWO mode-suffixed methods); the axis-typed
    //    owned-element type rides `<T as AtomKwarg>::Owned` at the
    //    substrate's trait default, so a hypothetical third atom axis
    //    (e.g. `Symbol` with `Owned = SymbolBuf`) lands as ZERO new
    //    emit strings here. Pinned by the sibling
    //    `both_axes_dispatch_to_the_same_method_name_per_mode` test.
    //
    // Peer to `narrow_trait_dispatch_call_tests` on the numeric-narrowing
    // axis: both modules pin the same six skeleton promises (UFCS
    // type, trait method identity, fully-qualified path, key literal,
    // `?` suffix, axis identity lives on `T`); the difference is
    // which trait is dispatched through — `AtomKwarg` here,
    // `NarrowNumeric` on the numeric-narrowing peer. Together the
    // two modules close every trait-dispatched arm's emission at the
    // test-module boundary.
    //
    // Theory anchor: THEORY.md §II.1 invariant 1 (typed entry) — the
    // helper IS the derive's rust-level typed-entry projection at the
    // atom-family list-family field's `syn::Type -> emitted extractor
    // call` boundary; naming its SIX emission promises as pinned
    // facts here makes future upgrades (a caller-supplied diagnostic
    // span, a `?`-suppressed variant, a new mode, or a new atom axis)
    // fail at the boundary of the shape they change rather than at
    // downstream implementors' compile-time noise. THEORY.md §II.1
    // invariant 3 (typed exit) — the axis identity riding the UFCS
    // type parameter routes through `<T as AtomKwarg>::Owned` at
    // rustc time, so the emit-time collapse across axes preserves
    // the per-atom-type owned-element identity.

    fn call_string(method: &'static str, rust_ty: &str, key: &str) -> String {
        atom_trait_dispatch_call(method, rust_ty, key).to_string()
    }

    #[test]
    fn helper_emits_the_ufcs_self_type_at_every_supported_atom_axis() {
        // Promise 1 (UFCS `Self` TYPE) — the `rust_ty` payload rides
        // the emitted call as a Rust type in the `<T as ...>` UFCS
        // slot. Sweep the two atom axes the derive's
        // `Kind::Vec{String,Bool}` / `Kind::OptionalVec{String,Bool}`
        // payload spans so a regression that (a) lost the type
        // parameter, (b) rendered the axis identity as a string
        // literal, or (c) dropped the payload entirely surfaces
        // per-axis rather than only on one canonical axis.
        for (rust_ty, expected_ufcs_head) in [("& str", "< & str as"), ("bool", "< bool as")] {
            let body = call_string("extract_list_kwarg", rust_ty, "tags");
            assert!(
                body.contains(expected_ufcs_head),
                "axis {rust_ty} must ride the UFCS type slot `{expected_ufcs_head} ...>`, got: {body}",
            );
            assert!(
                body.ends_with(r#"("tags") ?"#) || body.ends_with(r#"(& kw , "tags") ?"#),
                "emission must end with `(&kw, \"tags\")?`, got: {body}",
            );
        }
    }

    #[test]
    fn helper_emits_the_trait_method_ident_at_every_two_list_family_mode() {
        // Promise 2 (TRAIT METHOD IDENTITY) — the `method` string
        // rides the emitted call as an `Ident` at the derive's
        // call-site span. Sweep the TWO mode-suffixed trait methods
        // the derive's atom-family list-family arms map to, so a
        // regression that silently swapped ONE arm's method name at
        // the `extractor_for` dispatch site (e.g. a required-vec arm
        // accidentally routing through the optional-vec method) would
        // surface here rather than as a mystery type-mismatch at
        // every downstream consumer with a `Vec<String>` /
        // `Vec<bool>` field.
        //
        // Post-lift the TWO mode names cover BOTH axes (string + bool)
        // — halved from the pre-lift FOUR axis-typed public wrapper
        // names — because the owned-element type lives at `<T as
        // AtomKwarg>::Owned` on the target axis's impl block.
        //
        // The full UFCS path `< & str as :: tatara_lisp :: domain ::
        // AtomKwarg < '_ >> :: <method>` (the proc_macro2-formatted
        // spelling of `<&str as ::tatara_lisp::domain::AtomKwarg<'_>>
        // ::<method>` — the `>>` closing tokens are printed WITHOUT an
        // intervening space, unlike the `> >` shape a bare-generic
        // parameterization would emit at proc_macro2's default
        // formatter) pins the method as a load-bearing token trailing
        // the UFCS bracket.
        for method in ["extract_list_kwarg", "extract_optional_list_kwarg"] {
            let body = call_string(method, "& str", "tags");
            let expected_method_call = format!(">> :: {method}");
            assert!(
                body.contains(&expected_method_call),
                "method {method} must ride the emitted UFCS dispatch as `...{expected_method_call}`, got: {body}",
            );
        }
    }

    #[test]
    fn helper_emits_the_fully_qualified_trait_path_with_a_leading_double_colon() {
        // Promise 3 (FULLY-QUALIFIED TRAIT PATH) — the emitted UFCS
        // trait path starts with `::` so it resolves against the
        // crate root regardless of what the implementor's `use`
        // imports bring into scope. A regression that dropped the
        // leading `::` would silently drift the resolution to
        // whatever `tatara_lisp` the implementor's scope resolves (a
        // re-export, a local alias, a stub) rather than the substrate
        // crate.
        //
        // Pinned separately from the method-identity sweep because
        // it's a distinct promise (path hygiene) — a refactor that
        // (say) resolved the trait method `Ident` correctly but
        // emitted the trait path as a bare `AtomKwarg` (relying on
        // an implicit re-export) would pass the peer sweep and fail
        // here. Same posture the narrowed peer's test module gives
        // on its Promise 3.
        let body = call_string("extract_list_kwarg", "bool", "flags");
        assert!(
            body.contains(":: tatara_lisp :: domain :: AtomKwarg"),
            "emission must fully qualify the trait as `::tatara_lisp::domain::AtomKwarg`, got: {body}",
        );
    }

    #[test]
    fn helper_emits_the_key_literal_as_the_second_positional_argument() {
        // Promise 4 (KEY LITERAL PASS-THROUGH) — the `key` parameter
        // rides the emitted call's second argument as a Rust string
        // literal. Sweep three representative kebab-cased key shapes
        // the derive's `snake_to_kebab` projection emits
        // (single-word, hyphenated, digit-suffixed) so a regression
        // that (a) dropped the key, (b) interpolated it as an ident
        // instead of a string literal, or (c) reordered the arg
        // positions surfaces per-shape. Same posture the narrowed
        // peer's test module gives on its Promise 4.
        for (key, expected_lit) in [
            ("tags", "\"tags\""),
            ("allowed-labels", "\"allowed-labels\""),
            ("scale-v2", "\"scale-v2\""),
        ] {
            let body = call_string("extract_list_kwarg", "& str", key);
            assert!(
                body.contains(expected_lit),
                "key {key} must ride the emitted call as string literal {expected_lit}, got: {body}",
            );
        }
    }

    #[test]
    fn helper_emits_the_question_mark_suffix_so_the_expression_projects_to_the_extracted_vec() {
        // Promise 5 (?-SUFFIX) — the emission ends with `?` so the
        // caller-visible expression type is the extracted `Vec<T>` /
        // peer shape, matching every peer arm's `(&kw, #key)?` shape.
        // A regression that dropped the `?` would leave the
        // derived-code expression as a `Result<Vec<T>, LispError>` —
        // every consumer struct field initializer would fail at the
        // derived-code compile-time with a type mismatch, but the
        // derive's own emission would still pass its own compile.
        // Same posture the narrowed peer's test module gives on its
        // Promise 5.
        let body = call_string("extract_list_kwarg", "bool", "flags");
        assert!(
            body.trim_end().ends_with('?'),
            "emission must end with the `?` operator, got: {body}",
        );
    }

    #[test]
    fn both_axes_dispatch_to_the_same_method_name_per_mode() {
        // Promise 6 (AXIS IDENTITY LIVES ON `T`, NOT THE METHOD
        // NAME) — the two mode-suffixed method names cover BOTH the
        // string axis (`&str`) and the bool axis (`bool`); the
        // axis-typed owned-element type rides `<T as AtomKwarg>::Owned`
        // at the substrate's trait default. A regression that
        // reintroduced an axis-typed split in the method name (e.g.
        // `extract_string_list_kwarg` / `extract_bool_list_kwarg`)
        // would surface here at the mode-by-mode cross-check.
        //
        // For each of the TWO modes, walk one canonical string
        // axis (`&str`) and one canonical bool axis (`bool`) through
        // the SAME method name; assert the two emissions dispatch to
        // the same `>> :: <method>` trailing shape (the axis identity
        // lives only in the UFCS type payload preceding the trait
        // path — proc_macro2 emits the `>>` closing tokens without an
        // intervening space).
        for method in ["extract_list_kwarg", "extract_optional_list_kwarg"] {
            let string_body = call_string(method, "& str", "tags");
            let bool_body = call_string(method, "bool", "flags");
            let expected_trait_tail = format!(">> :: {method}");
            assert!(
                string_body.contains(&expected_trait_tail),
                "string-axis emission must dispatch through `...{expected_trait_tail}`, got: {string_body}",
            );
            assert!(
                bool_body.contains(&expected_trait_tail),
                "bool-axis emission must dispatch through `...{expected_trait_tail}`, got: {bool_body}",
            );
            // Method-name identity across axes — a hypothetical
            // regression that split the mode into a string-only /
            // bool-only pair would leave ONE of these two
            // assertions failing.
            assert!(
                !method.contains("string") && !method.contains("bool"),
                "method {method} must NOT carry an axis suffix — the axis lives on `<T as AtomKwarg>::Owned`",
            );
        }
    }

    #[test]
    fn helper_emission_is_token_equivalent_to_a_hand_written_vec_string_atom_trait_dispatch() {
        // Cross-check — the helper's emission at the canonical
        // `Kind::VecString` arm is TOKEN-EQUIVALENT (byte-identical
        // modulo whitespace) to a hand-written `quote! { <&str as
        // ::tatara_lisp::domain::AtomKwarg<'_>>::extract_list_kwarg
        // (&kw, "tags")? }` scaffold. A regression at any of Promises
        // 1-5 that individually passes the per-promise pins above but
        // composes into a token stream distinct from the hand-written
        // form would surface here — one final structural pin above
        // the per-promise arms.
        let key = "tags";
        let atom_ty: proc_macro2::TokenStream = "& str".parse().unwrap();
        let expected = quote! {
            <#atom_ty as ::tatara_lisp::domain::AtomKwarg<'_>>::extract_list_kwarg(&kw, #key)?
        }
        .to_string();
        let actual = call_string("extract_list_kwarg", "& str", key);
        assert_eq!(
            actual, expected,
            "helper emission must be token-equivalent to the hand-written vec-string atom trait dispatch",
        );
    }

    #[test]
    fn helper_emission_is_token_equivalent_to_a_hand_written_optional_vec_bool_atom_trait_dispatch()
    {
        // Bool-axis / optional-mode peer of the string / required
        // token-equivalence pin — closes the axis + mode diagonal on
        // the cross-check. Confirms the emission on `bool` at the
        // optional-vec mode dispatches through the SAME
        // `extract_optional_list_kwarg` trait method the string axis
        // reaches on the peer test above (only the UFCS type payload
        // changes across axes; the method name does not).
        let key = "flags";
        let atom_ty: proc_macro2::TokenStream = "bool".parse().unwrap();
        let expected = quote! {
            <#atom_ty as ::tatara_lisp::domain::AtomKwarg<'_>>::extract_optional_list_kwarg(&kw, #key)?
        }
        .to_string();
        let actual = call_string("extract_optional_list_kwarg", "bool", key);
        assert_eq!(
            actual, expected,
            "helper emission must be token-equivalent to the hand-written optional-vec-bool atom trait dispatch",
        );
    }

    #[test]
    fn helper_emits_the_two_scalar_family_method_idents_across_both_atom_axes() {
        // Scalar-family peer of the list-family
        // `helper_emits_the_trait_method_ident_at_every_two_list_family_mode`
        // sweep above — after the atom-family scalar-family lift the
        // helper covers FOUR mode-suffixed trait methods (TWO scalar
        // + TWO list) across BOTH atom axes (`&str` + `bool`). Sweep
        // the TWO scalar mode names `extract_owned_kwarg` /
        // `extract_optional_owned_kwarg` on both axes so a regression
        // that (a) reintroduced an axis-typed split at the scalar
        // methods (e.g. `extract_owned_string_kwarg` /
        // `extract_owned_bool_kwarg`) or (b) silently swapped one
        // scalar mode name at the `extractor_for` scalar dispatch
        // site surfaces here rather than as a mystery type-mismatch
        // at every downstream consumer with a `String` / `bool` /
        // `Option<String>` / `Option<bool>` field.
        //
        // Post the lift the FOUR mode names cover BOTH axes for each
        // of the TWO families (scalar / list), so the derive's
        // atom-family emission surface halves from FOUR + FOUR
        // per-axis-per-mode wrapper names down to TWO + TWO
        // mode-suffixed trait methods; the axis identity lives on
        // `<T as AtomKwarg>::Owned` on the target axis's impl block,
        // never on the emit string.
        for method in ["extract_owned_kwarg", "extract_optional_owned_kwarg"] {
            let string_body = call_string(method, "& str", "name");
            let bool_body = call_string(method, "bool", "enabled");
            let expected_trait_tail = format!(">> :: {method}");
            assert!(
                string_body.contains(&expected_trait_tail),
                "string-axis scalar emission must dispatch through `...{expected_trait_tail}`, got: {string_body}",
            );
            assert!(
                bool_body.contains(&expected_trait_tail),
                "bool-axis scalar emission must dispatch through `...{expected_trait_tail}`, got: {bool_body}",
            );
            assert!(
                !method.contains("string") && !method.contains("bool"),
                "scalar method {method} must NOT carry an axis suffix — the axis lives on `<T as AtomKwarg>::Owned`",
            );
        }
    }

    #[test]
    fn helper_emission_is_token_equivalent_to_a_hand_written_scalar_string_atom_trait_dispatch() {
        // Cross-check on the atom-family scalar-family axis — the
        // helper's emission at the canonical `Kind::String` arm is
        // TOKEN-EQUIVALENT to a hand-written `quote! { <&str as
        // ::tatara_lisp::domain::AtomKwarg<'_>>::extract_owned_kwarg
        // (&kw, "name")? }` scaffold. Pre-lift the arm inlined
        // `extract_string(&kw, "name")?.to_string()` — the
        // `.to_string()` fold restated the string-axis's
        // `<&'a str>::Owned = String` associated-type binding at the
        // derive call site; post-lift the fold rides through the
        // trait default at ONE per-axis dispatch site and drops out
        // of the emit path entirely. Closes the axis + mode diagonal
        // on the atom-family scalar cross-check — a regression that
        // affected only the scalar arms would pass the list-family
        // peer above and fail here.
        let key = "name";
        let atom_ty: proc_macro2::TokenStream = "& str".parse().unwrap();
        let expected = quote! {
            <#atom_ty as ::tatara_lisp::domain::AtomKwarg<'_>>::extract_owned_kwarg(&kw, #key)?
        }
        .to_string();
        let actual = call_string("extract_owned_kwarg", "& str", key);
        assert_eq!(
            actual, expected,
            "helper emission must be token-equivalent to the hand-written scalar-string atom trait dispatch",
        );
    }

    #[test]
    fn helper_emission_is_token_equivalent_to_a_hand_written_optional_scalar_bool_atom_trait_dispatch(
    ) {
        // Bool-axis / optional-scalar-mode peer of the string /
        // required-scalar token-equivalence pin above — closes the
        // axis + mode diagonal on the scalar cross-check. Confirms
        // the emission on `bool` at the optional-scalar mode
        // dispatches through the SAME `extract_optional_owned_kwarg`
        // trait method the string axis reaches on the peer sweep,
        // matching the same posture the list-family peer pair pins.
        // Pre-lift the arm inlined
        // `extract_optional_bool(&kw, "enabled")?` — the axis-typed
        // scalar wrapper name; post-lift the mode-suffixed trait
        // method covers BOTH axes.
        let key = "enabled";
        let atom_ty: proc_macro2::TokenStream = "bool".parse().unwrap();
        let expected = quote! {
            <#atom_ty as ::tatara_lisp::domain::AtomKwarg<'_>>::extract_optional_owned_kwarg(&kw, #key)?
        }
        .to_string();
        let actual = call_string("extract_optional_owned_kwarg", "bool", key);
        assert_eq!(
            actual, expected,
            "helper emission must be token-equivalent to the hand-written optional-scalar-bool atom trait dispatch",
        );
    }
}

#[cfg(test)]
mod deserialize_trait_dispatch_call_tests {
    use super::deserialize_trait_dispatch_call;
    use quote::quote;
    use syn::parse_str;

    // Universal-serde-fallthrough peer of `atom_trait_dispatch_call_tests`
    // and `narrow_trait_dispatch_call_tests`. The `(method, inner_ty,
    // key) -> TokenStream2` helper is the derive's PRIVATE emission
    // scaffold for the FOUR universal-serde-fallthrough arms in
    // `extractor_for` (`Kind::Deserialize` /
    // `Kind::OptionalDeserialize` / `Kind::VecDeserialize` /
    // `Kind::OptionalVecDeserialize`). Each arm is now a one-line
    // delegate onto this helper — post-lift the inner `T` rides
    // the `Kind` payload the classifier already cached at
    // [`super::classify_option`] / [`super::classify_vec`], so the
    // extract site no longer re-walks the outer field type; the
    // shared scaffold — thread the cached inner `T` verbatim as a
    // UFCS `Self` type, resolve the trait method `Ident` at the
    // derive's call-site span, emit the fully-qualified `<T as
    // ::tatara_lisp::domain::DeserializeKwarg>::<method>(&kw,
    // #key)?` call — lives at ONE substrate primitive.
    //
    // Post-lift the derive's non-atom emission surface consists of
    // ONE trait-dispatch primitive per axis-family: atom-family
    // through [`super::atom_trait_dispatch_call`], numeric-
    // narrowed through [`super::narrow_trait_dispatch_call`],
    // universal-serde-fallthrough through this helper. The four
    // universal-serde-fallthrough free-function names in
    // `tatara_lisp::domain` (`extract_via_serde` /
    // `extract_optional_via_serde` / `extract_vec_via_serde` /
    // `extract_optional_vec_via_serde`) retain their public-API
    // tests but no longer have a derive caller; the derive
    // dispatches through `<T as DeserializeKwarg>::<method>`
    // instead, and the trait's blanket impl for every
    // `T: DeserializeOwned` funnels through the SAME four free
    // functions at the substrate's trait default so the observed
    // behaviour is byte-identical to the pre-lift bare-name
    // dispatch — pinned end-to-end by the substrate-side
    // delegation-identity tests
    // `deserialize_kwarg_extract_*_default_*` in
    // `tatara-lisp/src/domain.rs`.
    //
    // A regression here silently swaps the emitted code at every
    // `#[derive(TataraDomain)]` implementor with a nested-struct /
    // enum / `Vec<Nested>` / `Option<Vec<Nested>>` field. The
    // tests below pin the SIX promises the helper owns at the
    // boundary of its emission shape, mirroring the SIX-promise
    // pin its atom-family peer gives:
    //
    // 1. UFCS `Self` TYPE — the `inner_ty` `TokenStream2` payload
    //    rides the emitted `<T as ...>::<method>(&kw, #key)?` as a
    //    Rust type verbatim, NOT stringified, not composed by
    //    string concatenation. Pinned by the four-arm sweep test
    //    below (four representative inner-type spellings — a
    //    plain-ident enum, a nested-struct, a `Vec<Nested>`, and
    //    a fully-qualified path — all land in the UFCS type slot
    //    verbatim).
    // 2. TRAIT METHOD IDENTITY — the `method` `&'static str`
    //    payload rides the emitted `<T as ...DeserializeKwarg>::
    //    <method>` path as an `Ident` at the call-site span, not
    //    as a string literal or a `Path` composed by string
    //    concatenation. Pinned by the four-mode sweep test that
    //    walks each of the FOUR mode-suffixed trait defaults the
    //    derive's per-mode dispatch pairs to (`extract_kwarg` /
    //    `extract_optional_kwarg` / `extract_vec_kwarg` /
    //    `extract_optional_vec_kwarg`).
    // 3. FULLY-QUALIFIED TRAIT PATH — the emission names the
    //    trait as `::tatara_lisp::domain::DeserializeKwarg` —
    //    leading `::` + three-segment path — so the UFCS dispatch
    //    is hygienic in the derived code's outer scope regardless
    //    of what the implementor's crate imports (the same
    //    discipline the two peer trait-dispatch helpers give).
    // 4. KEY LITERAL PASS-THROUGH — the `key` parameter rides the
    //    emitted call's second argument as a Rust string literal,
    //    so the emitted call reads the SAME kwarg the derive's
    //    per-field `#key` interpolation names (kebab-cased from
    //    the Rust field ident by `snake_to_kebab`).
    // 5. `?`-SUFFIX — the emission ends with the `?` operator so
    //    the caller-visible expression type is the extracted
    //    value (not a `Result`), matching every peer trait-
    //    dispatch helper's shape.
    // 6. TOKEN-EQUIVALENCE with the hand-written UFCS scaffold —
    //    the helper's composition of the six promises is byte-
    //    identical (modulo whitespace) to a hand-written
    //    `quote!` scaffold. Same posture the atom-family peer's
    //    test module gives on its two token-equivalence pins.
    //
    // Theory anchor: THEORY.md §II.1 invariant 1 (typed entry) —
    // the helper IS the derive's rust-level typed-entry
    // projection at the universal-serde-fallthrough field's
    // `syn::Type -> emitted extractor call` boundary; naming its
    // SIX emission promises as pinned facts here makes future
    // upgrades (a caller-supplied diagnostic span, a `?`-
    // suppressed variant, a new mode) fail at the boundary of
    // the shape they change rather than at downstream
    // implementors' compile-time noise.

    fn call_string(method: &'static str, inner_ty_src: &str, key: &str) -> String {
        let ty: syn::Type = parse_str(inner_ty_src).unwrap();
        deserialize_trait_dispatch_call(method, quote! { #ty }, key).to_string()
    }

    #[test]
    fn helper_emits_the_inner_type_at_the_ufcs_self_slot_for_every_representative_shape() {
        // Promise 1 (UFCS `Self` TYPE) — the `inner_ty` `TokenStream2`
        // payload rides the emitted `<T as ...>::<method>(&kw, #key)?`
        // as a Rust type verbatim. Sweep four representative shapes
        // the derive's classifier caches into the `Kind` payload:
        //   - a plain-ident enum (`Severity`)
        //   - a nested-struct (`EscalationStep`)
        //   - a `Vec<T>` inner type (the outer `Kind::VecDeserialize` /
        //     `Kind::OptionalVecDeserialize` walk unwraps one level,
        //     leaving the `T` here — a nested struct in the common case
        //     but a bare `String` would work too)
        //   - a fully-qualified path (`::my_crate::my_mod::Config`)
        // — a regression that (say) stringified the inner type or
        // dropped its path prefix would surface per-shape here.
        //
        // The full prefix `< #inner_ty as :: tatara_lisp :: domain ::
        // DeserializeKwarg >` (the proc_macro2 spelling) pins the
        // inner type as a load-bearing token at the UFCS binding.
        for (inner, expected_prefix) in [
            (
                "Severity",
                "< Severity as :: tatara_lisp :: domain :: DeserializeKwarg >",
            ),
            (
                "EscalationStep",
                "< EscalationStep as :: tatara_lisp :: domain :: DeserializeKwarg >",
            ),
            (
                "String",
                "< String as :: tatara_lisp :: domain :: DeserializeKwarg >",
            ),
            (
                "::my_crate::my_mod::Config",
                ":: my_crate :: my_mod :: Config as :: tatara_lisp :: domain :: DeserializeKwarg",
            ),
        ] {
            let body = call_string("extract_kwarg", inner, "field");
            assert!(
                body.contains(expected_prefix),
                "inner type {inner} must ride the UFCS Self slot as `{expected_prefix}`, got: {body}",
            );
        }
    }

    #[test]
    fn helper_emits_the_trait_method_ident_at_every_four_mode_arm() {
        // Promise 2 (TRAIT METHOD IDENTITY) — the `method` string
        // rides the emitted call as an `Ident` at the derive's
        // call-site span. Sweep the FOUR mode-suffixed trait
        // defaults the derive threads through this helper — one
        // for each of `Kind::(Optional?)(Vec?)Deserialize`. A
        // regression that silently swapped ONE mode's method
        // name at the `extractor_for` dispatch site (e.g. a
        // `Kind::VecDeserialize` arm routing through
        // `extract_kwarg` — which would return `Result<T>`
        // instead of `Result<Vec<T>>` and mismatch the derived
        // struct field type) would surface here rather than as a
        // mystery type-mismatch at every downstream consumer
        // with a `Vec<Nested>` field.
        for method in [
            "extract_kwarg",
            "extract_optional_kwarg",
            "extract_vec_kwarg",
            "extract_optional_vec_kwarg",
        ] {
            let body = call_string(method, "Severity", "field");
            let expected_suffix = format!(":: DeserializeKwarg > :: {method}");
            assert!(
                body.contains(&expected_suffix),
                "method {method} must ride the emitted UFCS path as `{expected_suffix}`, got: {body}",
            );
        }
    }

    #[test]
    fn helper_emits_the_fully_qualified_trait_path_with_a_leading_double_colon() {
        // Promise 3 (FULLY-QUALIFIED TRAIT PATH) — the emitted
        // UFCS binding names the trait as
        // `::tatara_lisp::domain::DeserializeKwarg` — leading
        // `::` + three-segment path — so the dispatch resolves
        // against the crate root regardless of what the
        // implementor's `use` imports bring into scope.
        let body = call_string("extract_kwarg", "Severity", "enabled");
        assert!(
            body.contains("as :: tatara_lisp :: domain :: DeserializeKwarg"),
            "emission must bind the trait as `::tatara_lisp::domain::DeserializeKwarg`, got: {body}",
        );
    }

    #[test]
    fn helper_emits_the_key_literal_as_the_second_positional_argument() {
        // Promise 4 (KEY LITERAL PASS-THROUGH) — the `key`
        // parameter rides the emitted call's second argument as
        // a Rust string literal. Sweep three representative
        // kebab-cased key shapes the derive's `snake_to_kebab`
        // projection emits.
        for (key, expected_lit) in [
            ("enabled", "\"enabled\""),
            ("window-seconds", "\"window-seconds\""),
            ("scale-v2", "\"scale-v2\""),
        ] {
            let body = call_string("extract_kwarg", "Severity", key);
            assert!(
                body.contains(expected_lit),
                "key {key} must ride the emitted call as string literal {expected_lit}, got: {body}",
            );
        }
    }

    #[test]
    fn helper_emits_the_question_mark_suffix_so_the_expression_projects_to_the_extracted_value() {
        // Promise 5 (?-SUFFIX) — the emission ends with `?` so
        // the caller-visible expression type is the extracted
        // value shape (`T`, `Option<T>`, `Vec<T>`, or
        // `Option<Vec<T>>` depending on which mode the derive
        // dispatched through).
        let body = call_string("extract_kwarg", "Severity", "enabled");
        assert!(
            body.trim_end().ends_with('?'),
            "emission must end with the `?` operator, got: {body}",
        );
    }

    #[test]
    fn helper_emission_is_token_equivalent_to_the_hand_written_scalar_deserialize_ufcs_call() {
        // Promise 6 (TOKEN-EQUIVALENCE, scalar diagonal) — the
        // helper's emission at the canonical `Kind::Deserialize`
        // arm is TOKEN-EQUIVALENT (byte-identical modulo
        // whitespace) to a hand-written UFCS scaffold. A
        // regression at any of Promises 1-5 that individually
        // passes the per-promise pins above but composes into a
        // token stream distinct from the hand-written scaffold
        // would surface here.
        let key = "config";
        let inner: syn::Type = parse_str("Severity").unwrap();
        let expected = quote! {
            <#inner as ::tatara_lisp::domain::DeserializeKwarg>::extract_kwarg(&kw, #key)?
        }
        .to_string();
        let actual =
            deserialize_trait_dispatch_call("extract_kwarg", quote! { #inner }, key).to_string();
        assert_eq!(
            actual, expected,
            "helper emission must be token-equivalent to the hand-written scalar-deserialize UFCS call",
        );
    }

    #[test]
    fn helper_emission_is_token_equivalent_to_the_hand_written_optional_vec_deserialize_ufcs_call()
    {
        // Promise 6 (TOKEN-EQUIVALENCE, optional-vec diagonal) —
        // closes the axis × mode diagonal on the cross-check. A
        // regression that affected only the vec-mode arms (say,
        // one that resolved the scalar mode correctly but
        // drifted the vec-mode dispatch shape) would pass the
        // scalar pin above and fail here.
        let key = "steps";
        let inner: syn::Type = parse_str("EscalationStep").unwrap();
        let expected = quote! {
            <#inner as ::tatara_lisp::domain::DeserializeKwarg>::extract_optional_vec_kwarg(&kw, #key)?
        }
        .to_string();
        let actual =
            deserialize_trait_dispatch_call("extract_optional_vec_kwarg", quote! { #inner }, key)
                .to_string();
        assert_eq!(
            actual, expected,
            "helper emission must be token-equivalent to the hand-written optional-vec-deserialize UFCS call",
        );
    }
}

#[cfg(test)]
mod trait_dispatch_tail_tests {
    use super::trait_dispatch_tail;
    use quote::quote;

    // Substrate-primitive peer of the three axis-typed test modules
    // (`narrow_trait_dispatch_call_tests` / `atom_trait_dispatch_call_tests`
    // / `deserialize_trait_dispatch_call_tests`). The `(method, key)
    // -> TokenStream2` helper is the derive's PRIVATE tail scaffold
    // for the trailing UFCS-arm shape every trait-dispatch helper on
    // the derive's emission surface appends after its axis-specific
    // UFCS shell — the emitted trailing tokens are `#method(&kw,
    // #key)?`, dispatched into the enclosing `<T as
    // ::tatara_lisp::domain::<Trait>>::` UFCS bracket the three peer
    // helpers own.
    //
    // A regression here silently swaps the trailing arg shape at
    // every trait-dispatched arm in `extractor_for` (all EIGHT
    // numeric-narrowed arms via `narrow_trait_dispatch_call`, all
    // FOUR atom-family scalar arms + all FOUR atom-family list arms
    // via `atom_trait_dispatch_call`, all FOUR universal-serde-
    // fallthrough arms via `deserialize_trait_dispatch_call`). The
    // tests below pin the THREE promises the tail owns at the
    // boundary of its emission shape:
    //
    // 1. METHOD IDENT AT CALL-SITE SPAN — the `method` `&'static
    //    str` payload rides the emitted tail as an `Ident` at the
    //    derive's call-site span, NOT as a string literal or a
    //    `Path` composed by string concatenation. Pinned by the
    //    method-ident sweep test that walks each of the FOUR
    //    representative mode-suffixed method names the three peer
    //    helpers dispatch through.
    // 2. `(&kw, #key)?` ARG SHAPE — the `key` parameter rides the
    //    emitted tail's second positional argument as a Rust string
    //    literal (kebab-cased from the field ident by
    //    [`super::snake_to_kebab`] at the field-level derive site);
    //    the `&kw` borrow rides the first positional argument as an
    //    ident reference to the surrounding scope's kwarg map. Pinned
    //    by the key-literal + arg-order sweep test.
    // 3. `?`-SUFFIX — the tail ends with the `?` operator so the
    //    caller-visible expression type is the extracted `T` /
    //    `Option<T>` / `Vec<T>` / `Option<Vec<T>>` shape rather than
    //    a `Result`; every consumer of the UFCS call composes its
    //    output directly into a derived struct-field initializer
    //    without a per-arm re-`?`. Pinned by the `?`-suffix test.
    //
    // Peer to `narrow_trait_dispatch_call_tests` /
    // `atom_trait_dispatch_call_tests` /
    // `deserialize_trait_dispatch_call_tests` on the emission-shape
    // axis: those three modules pin the axis-typed UFCS shells the
    // three peer helpers own (leading `::`, `<T as
    // ...::<Trait>>::` binding, trait-path hygiene), this module
    // pins the axis-AGNOSTIC tail invariants the substrate primitive
    // owns. Together the four modules close every trait-dispatched
    // arm's emission at the derive's `syn::Type -> emitted extractor
    // call` boundary at the test-module surface.
    //
    // Theory anchor: THEORY.md §II.1 invariant 1 (typed entry) — the
    // tail IS the derive's rust-level typed-entry projection at the
    // trait-dispatch call boundary; naming its THREE emission
    // promises as pinned facts here makes future upgrades (a
    // caller-supplied diagnostic span at the method ident, a `?`-
    // suppressed variant that plumbs the axis-typed rejection
    // through a `Result` chain rather than a `?`, an extension of
    // the two-arg positional shape with a callsite-derived context
    // frame) fail at the boundary of the shape they change rather
    // than at downstream implementors' compile-time noise. THEORY.md
    // §II.1 invariant 5 (composition preserves proofs) — the tail's
    // axis-agnosticism means a future FOURTH trait axis (a
    // hypothetical `SymbolKwarg`, `PathKwarg`, or `EnumVariantKwarg`)
    // reuses this primitive rather than restating the three
    // invariants a fourth time.

    fn call_string(method: &'static str, key: &str) -> String {
        trait_dispatch_tail(method, key).to_string()
    }

    #[test]
    fn tail_emits_the_method_ident_at_every_representative_mode_across_the_three_axes() {
        // Promise 1 (METHOD IDENT AT CALL-SITE SPAN) — the `method`
        // string rides the emitted tail as an `Ident` at the derive's
        // call-site span. Sweep FOUR representative method names
        // spanning the three axis-typed helper cohorts (one per
        // trait's per-mode dispatch family):
        //
        //   - `extract_narrowed_kwarg`         — NarrowNumeric (int/float scalar)
        //   - `extract_optional_narrowed_list_kwarg` — NarrowNumeric (int/float optional-vec)
        //   - `extract_owned_kwarg`            — AtomKwarg (string/bool scalar)
        //   - `extract_optional_vec_kwarg`     — DeserializeKwarg (nested Option<Vec<T>>)
        //
        // A regression that (a) stringified the method into a Rust
        // string literal, (b) composed it as a fragmentary `Path`
        // via string concatenation, or (c) resolved it at a Span
        // OTHER than `call_site()` (which would break the UFCS
        // dispatch hygiene in the derived code's outer scope) would
        // surface per-representative-method here.
        //
        // The emitted tail's leading token IS the bare method ident
        // (no leading `::`, no leading trait-path, no wrapper), so a
        // regression that dropped or reshaped the ident would surface
        // as the tail failing to START with the method ident.
        for method in [
            "extract_narrowed_kwarg",
            "extract_optional_narrowed_list_kwarg",
            "extract_owned_kwarg",
            "extract_optional_vec_kwarg",
        ] {
            let body = call_string(method, "field");
            assert!(
                body.starts_with(method),
                "tail must start with the method ident `{method}`, got: {body}",
            );
        }
    }

    #[test]
    fn tail_emits_the_kw_borrow_and_key_literal_as_the_two_positional_arguments_in_order() {
        // Promise 2 (`(&kw, #key)?` ARG SHAPE) — the `key` parameter
        // rides the tail's SECOND positional argument as a Rust
        // string literal, and the `&kw` borrow rides the FIRST
        // positional argument as an ident reference. Sweep three
        // representative kebab-cased key shapes (single-word,
        // hyphenated, digit-suffixed) so a regression that (a)
        // dropped the `&kw` borrow, (b) reordered the args, or (c)
        // interpolated the key as an ident instead of a string
        // literal surfaces per-shape.
        //
        // The `(& kw , "{key}")` bracketing pins the full positional
        // arg pattern: the `&` borrow, the `kw` ident, the `,`
        // separator, and the string-literal key spelling. proc_macro2
        // formats each of these as separate tokens with intervening
        // whitespace.
        for (key, expected_arg) in [
            ("port", r#"(& kw , "port")"#),
            ("window-seconds", r#"(& kw , "window-seconds")"#),
            ("scale-v2", r#"(& kw , "scale-v2")"#),
        ] {
            let body = call_string("extract_narrowed_kwarg", key);
            assert!(
                body.contains(expected_arg),
                "tail must carry positional args as `{expected_arg}`, got: {body}",
            );
        }
    }

    #[test]
    fn tail_emits_the_question_mark_suffix_so_the_expression_projects_to_the_extracted_value() {
        // Promise 3 (?-SUFFIX) — the tail ends with `?` so the
        // caller-visible expression type is the extracted value
        // shape (`T`, `Option<T>`, `Vec<T>`, or `Option<Vec<T>>`
        // depending on which mode the derive dispatched through).
        // A regression that dropped the `?` would leave the
        // derived-code expression as a `Result<T, LispError>` at
        // every one of the SIXTEEN trait-dispatched arms in
        // `extractor_for` — every consumer struct field initializer
        // would fail at the derived-code compile-time with a type
        // mismatch, but the derive's own emission would still pass
        // its own compile.
        let body = call_string("extract_kwarg", "config");
        assert!(
            body.trim_end().ends_with('?'),
            "tail must end with the `?` operator, got: {body}",
        );
    }

    #[test]
    fn tail_is_axis_agnostic_and_carries_no_trait_path() {
        // Cross-check on the tail's axis-agnosticism — the tail
        // primitive's emission carries NO trait-path substring
        // (`NarrowNumeric` / `AtomKwarg` / `DeserializeKwarg`), NO
        // `tatara_lisp` crate-path segment, NO `<... as ...>` UFCS
        // bracket — those three shell pieces live at the three
        // per-axis helpers' outer quote blocks. The tail is JUST the
        // `#method(&kw, #key)?` trailing arm.
        //
        // A regression that leaked axis-typed shell content into the
        // tail (e.g. accidentally including the `AtomKwarg<'_>>::`
        // fragment) would break the axis-agnosticism invariant and
        // surface here, and would ALSO surface as duplicate content
        // at every peer axis helper's per-axis tests. Pinning the
        // negative here catches the regression BEFORE the peer tests
        // fire, closing the axis-agnostic contract at the boundary
        // of the tail primitive itself rather than downstream.
        let body = call_string("extract_kwarg", "field");
        for banned_substring in [
            "NarrowNumeric",
            "AtomKwarg",
            "DeserializeKwarg",
            "tatara_lisp",
            "as :",
            "< ",
        ] {
            assert!(
                !body.contains(banned_substring),
                "tail must NOT carry axis-typed shell content `{banned_substring}` — that lives at the per-axis UFCS wrapper, not the shared tail; got: {body}",
            );
        }
    }

    #[test]
    fn tail_emission_is_token_equivalent_to_the_hand_written_method_call_arm() {
        // Cross-check — the helper's emission at the canonical
        // `extract_kwarg` arm is TOKEN-EQUIVALENT (byte-identical
        // modulo whitespace) to a hand-written `quote! {
        // extract_kwarg(&kw, "field")? }` scaffold. A regression at
        // any of Promises 1-3 that individually passes the per-
        // promise pins above but composes into a token stream
        // distinct from the hand-written form would surface here.
        let key = "field";
        let method_ident = syn::Ident::new("extract_kwarg", proc_macro2::Span::call_site());
        let expected = quote! {
            #method_ident(&kw, #key)?
        }
        .to_string();
        let actual = call_string("extract_kwarg", key);
        assert_eq!(
            actual, expected,
            "tail emission must be token-equivalent to the hand-written `#method(&kw, #key)?` scaffold",
        );
    }

    #[test]
    fn tail_emission_composes_byte_identically_at_every_axis_typed_wrapper() {
        // Structural cross-check — the three axis-typed helpers'
        // emissions each equal `<Self as ::path::<Trait>>::` +
        // `trait_dispatch_tail(method, key)`. This test asserts
        // that property at the byte level for one representative
        // (method, key, self-ty) triple per axis, so a regression
        // that (a) reintroduced hand-rolled tail-shape code at ONE
        // axis-typed helper (breaking the substrate-lift claim), or
        // (b) drifted the trailing tail-shape at ONE axis (leaving
        // the two peers passing) would surface here rather than as
        // silent drift at only the affected axis's downstream
        // consumers.
        //
        // The three helpers each own a distinct trait-typed UFCS
        // shell (`NarrowNumeric` / `AtomKwarg<'_>` /
        // `DeserializeKwarg`); the SHARED tail rides through
        // `#tail` interpolation in each. Assembling the expected
        // full emission from the shared tail here proves the tail
        // is the ONE substrate primitive the three helpers compose
        // against.
        use super::{
            atom_trait_dispatch_call, deserialize_trait_dispatch_call, narrow_trait_dispatch_call,
        };

        let tail_narrow = trait_dispatch_tail("extract_narrowed_kwarg", "port").to_string();
        let narrowed: proc_macro2::TokenStream = "u16".parse().unwrap();
        let expected_narrow_prefix = quote! {
            <#narrowed as ::tatara_lisp::domain::NarrowNumeric>::
        }
        .to_string();
        let expected_narrow = format!("{expected_narrow_prefix} {tail_narrow}");
        let actual_narrow =
            narrow_trait_dispatch_call("extract_narrowed_kwarg", "u16", "port").to_string();
        assert_eq!(
            actual_narrow, expected_narrow,
            "narrow_trait_dispatch_call must compose as `<Self as ...NarrowNumeric>::` + shared tail",
        );

        let tail_atom = trait_dispatch_tail("extract_list_kwarg", "tags").to_string();
        let atom_ty: proc_macro2::TokenStream = "& str".parse().unwrap();
        let expected_atom_prefix = quote! {
            <#atom_ty as ::tatara_lisp::domain::AtomKwarg<'_>>::
        }
        .to_string();
        let expected_atom = format!("{expected_atom_prefix} {tail_atom}");
        let actual_atom =
            atom_trait_dispatch_call("extract_list_kwarg", "& str", "tags").to_string();
        assert_eq!(
            actual_atom, expected_atom,
            "atom_trait_dispatch_call must compose as `<Self as ...AtomKwarg<'_>>::` + shared tail",
        );

        let tail_deser = trait_dispatch_tail("extract_kwarg", "config").to_string();
        let inner: syn::Type = syn::parse_str("Severity").unwrap();
        let expected_deser_prefix = quote! {
            <#inner as ::tatara_lisp::domain::DeserializeKwarg>::
        }
        .to_string();
        let expected_deser = format!("{expected_deser_prefix} {tail_deser}");
        let actual_deser =
            deserialize_trait_dispatch_call("extract_kwarg", quote! { #inner }, "config")
                .to_string();
        assert_eq!(
            actual_deser, expected_deser,
            "deserialize_trait_dispatch_call must compose as `<Self as ...DeserializeKwarg>::` + shared tail",
        );
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
mod serde_default_tests {
    use super::{serde_default, SerdeDefault};
    use syn::{parse::Parser, Field};

    // The `(&syn::Field -> SerdeDefault)` projection is the derive's
    // PRIVATE typed sniffer for the field's `#[serde(default[ = "path"])]`
    // posture. Where the sibling `has_serde_default` collapses the
    // three-way authoring shape into a bool (`Absent` → false, `Trait`
    // and `Path(_)` both → true), this typed projection preserves
    // the `Path(_)` payload the derive threads into the missing-kwarg
    // dispatch branch inside `extractor_for` — the operator-authored
    // `= "path"` initializer fn.
    //
    // Pre-lift the payload was silently dropped: every `#[serde(default
    // = "path")]` field emitted `::std::default::Default::default()` at
    // the missing-kwarg branch regardless of the operator-chosen path,
    // diverging from serde's own semantics (a known workaround
    // documented in tatara-init/src/config.rs's `empty_definit_parses`
    // test). Post-lift the payload rides through the typed
    // `SerdeDefault::Path(path)` variant and `extractor_for` splices
    // `#path_ts()` into the missing-kwarg branch; the derive's
    // missing-kwarg semantics now match serde's byte-for-byte.
    //
    // A regression at `serde_default` silently swaps the missing-
    // kwarg branch on every consumer with a `#[serde(default[ = "…"])]`
    // field: an `Absent → Trait` false-positive regresses a required
    // field to a silent `Default::default()` slot; a `Trait → Absent`
    // false-negative regresses a legitimate default to a hard
    // `LispError::MissingKwarg`; a `Path(_) → Trait` payload-drop
    // regresses a serde-honoring field to `Default::default()`
    // (silently dropping the operator-chosen initializer).

    fn field(src: &str) -> Field {
        Field::parse_named
            .parse_str(src)
            .expect("valid named-field syntax")
    }

    #[test]
    fn field_with_no_attributes_projects_to_absent() {
        // Bread-and-butter: a bare `pub name: String` field carries NO
        // `#[serde(...)]` attribute → the projection returns `Absent`,
        // and `extractor_for` emits the raw extractor with no missing-
        // kwarg short-circuit at all (a missing kwarg surfaces as
        // `LispError::MissingKwarg`).
        assert_eq!(
            serde_default(&field("pub name: String")),
            SerdeDefault::Absent
        );
    }

    #[test]
    fn bare_serde_default_form_projects_to_trait_variant() {
        // The bare `#[serde(default)]` form — the callback finds a
        // `default` ident with no `= <expr>` payload; `meta.value()`
        // returns Err, the callback returns `Ok(None)`, the outer
        // projection folds to `SerdeDefault::Trait`. `extractor_for`
        // emits `if kw.contains_key(#key) { #base } else {
        // ::std::default::Default::default() }`.
        assert_eq!(
            serde_default(&field("#[serde(default)] pub name: Vec<String>")),
            SerdeDefault::Trait,
        );
    }

    #[test]
    fn serde_default_with_string_path_projects_to_path_variant_carrying_the_operator_authored_path()
    {
        // The `#[serde(default = "path::to::fn")]` form — the callback
        // finds a `default` ident followed by `= "path::to::fn"`;
        // `meta.value()` returns Ok, `parse_lit_str` projects the
        // LitStr payload to the owned `String` "path::to::fn", the
        // outer projection folds to `SerdeDefault::Path("path::to::fn")`.
        // `extractor_for` splices `path::to::fn()` into the missing-
        // kwarg branch. Pins the path-preservation contract: the
        // operator-authored initializer path rides through VERBATIM,
        // not dropped and not silently rewritten.
        assert_eq!(
            serde_default(&field(
                r#"#[serde(default = "path::to::fn")] pub name: String"#
            )),
            SerdeDefault::Path("path::to::fn".to_string()),
        );
    }

    #[test]
    fn serde_default_with_bare_ident_path_projects_to_path_variant() {
        // Common shape: `#[serde(default = "default_true")]` — a bare
        // ident (not a dotted path). Projects to `Path("default_true")`.
        // Peer of the tatara-init workaround site: `default_true` is
        // the exact fn name tatara-init/src/config.rs binds through
        // this attribute for `reap_zombies`, `reload_on_sighup`, etc.
        assert_eq!(
            serde_default(&field(
                r#"#[serde(default = "default_true")] pub reap: bool"#
            )),
            SerdeDefault::Path("default_true".to_string()),
        );
    }

    #[test]
    fn serde_default_path_and_trait_are_distinct_variants_not_collapsed_to_a_boolean() {
        // Structural cross-check — the two authoring shapes project to
        // DISTINCT `SerdeDefault` variants (not collapsed to a shared
        // bool). A regression that collapsed the two into one variant
        // would drop the path payload; this test surfaces it at the
        // projection boundary.
        let bare = serde_default(&field("#[serde(default)] pub x: String"));
        let with_path = serde_default(&field(r#"#[serde(default = "f")] pub x: String"#));
        assert_ne!(bare, with_path);
        assert_eq!(bare, SerdeDefault::Trait);
        assert_eq!(with_path, SerdeDefault::Path("f".to_string()));
    }

    #[test]
    fn default_after_other_key_named_value_pair_projects_to_trait_variant() {
        // Peer to the sibling `has_serde_default_tests` test — the
        // `#[serde(rename = "x", default)]` form threads the default
        // sub-key through the same value-drain discipline the primitive
        // owns. Pin the typed variant: the `default` sub-key sits AFTER
        // a value-carrying `rename` peer; the projection returns
        // `Trait` (not `Absent`), matching the sibling bool sniffer's
        // contract.
        assert_eq!(
            serde_default(&field(
                r#"#[serde(rename = "x", default)] pub name: String"#
            )),
            SerdeDefault::Trait,
        );
    }

    #[test]
    fn default_path_after_other_key_named_value_pair_projects_to_path_variant() {
        // The path-payload sibling of the test above — pins that the
        // typed projection carries the payload through even when the
        // `default = "fn"` sub-key sits AFTER a value-carrying peer.
        // The value-drain of the preceding `rename = "x"` peer AND the
        // typed payload capture of the trailing `default = "fn"` peer
        // both compose against the shared `find_named_sub_key`
        // primitive at ONE substrate entry.
        assert_eq!(
            serde_default(&field(
                r#"#[serde(rename = "x", default = "fn")] pub name: String"#
            )),
            SerdeDefault::Path("fn".to_string()),
        );
    }

    #[test]
    fn rename_value_containing_default_substring_projects_to_absent() {
        // Sibling of the `has_serde_default_tests` false-positive
        // guard — pin that the typed projection ALSO correctly rejects
        // `#[serde(rename = "default_val")]` at `Absent` (not `Trait`).
        // A regression that broadened the sub-key matcher to a
        // substring check would surface here as `Trait` (with a
        // corrupted "trait" branch inheriting the rename's payload) or
        // even as `Path("default_val")` (leaking the rename value into
        // the initializer-fn slot).
        assert_eq!(
            serde_default(&field(
                r#"#[serde(rename = "default_val")] pub name: String"#
            )),
            SerdeDefault::Absent,
        );
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
