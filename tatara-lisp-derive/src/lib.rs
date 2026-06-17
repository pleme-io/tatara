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
//!             threshold: extract_float(&kw, "threshold")?,
//!             window_seconds: extract_optional_int(&kw, "window-seconds")?,
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
        return syn::Error::new_spanned(
            &name,
            "ClosedSet may only be derived on enums (the closed-set-enum idiom)",
        )
        .to_compile_error()
        .into();
    }

    let cfg = match parse_closed_set_attrs(&input.attrs, &name) {
        Ok(c) => c,
        Err(err) => return err.to_compile_error().into(),
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
            if meta.path.is_ident("via") {
                let value = meta.value()?;
                let s: LitStr = value.parse()?;
                via = Some(s.value());
                Ok(())
            } else if meta.path.is_ident("unknown") {
                let value = meta.value()?;
                let s: LitStr = value.parse()?;
                unknown = Some(s.value());
                Ok(())
            } else if meta.path.is_ident("no_from_str") {
                no_from_str = true;
                Ok(())
            } else if meta.path.is_ident("generate_unknown") {
                // Both bare `generate_unknown` (auto-derived label)
                // and `generate_unknown = "explicit label"` (pinned
                // label) sit on ONE attribute key — the parser
                // dispatches on whether `meta.value()` succeeds so the
                // attribute surface stays single-keyed (no
                // `auto_label`/`label` bifurcation that would force
                // the operator to think about which of two
                // attributes is canonical).
                generate_unknown = match meta.value() {
                    Ok(value) => {
                        let s: LitStr = value.parse()?;
                        GenerateUnknown::Explicit(s.value())
                    }
                    Err(_) => GenerateUnknown::Auto,
                };
                Ok(())
            } else if meta.path.is_ident("display") {
                display = true;
                Ok(())
            } else if meta.path.is_ident("set_label") {
                let value = meta.value()?;
                let s: LitStr = value.parse()?;
                set_label = Some(s.value());
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
                return syn::Error::new_spanned(
                    &name,
                    "TataraDomain requires a struct with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&name, "TataraDomain may only be derived on structs")
                .to_compile_error()
                .into();
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
            Err(err) => {
                return syn::Error::new_spanned(&field.ty, err)
                    .to_compile_error()
                    .into();
            }
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
    for attr in attrs {
        if !attr.path().is_ident("tatara") {
            continue;
        }
        let Meta::List(list) = &attr.meta else {
            continue;
        };
        let mut found: Option<String> = None;
        let _ = list.parse_nested_meta(|meta| {
            if meta.path.is_ident("keyword") {
                let value = meta.value()?;
                let s: LitStr = value.parse()?;
                found = Some(s.value());
            }
            Ok(())
        });
        if found.is_some() {
            return found;
        }
    }
    None
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
fn has_serde_default(field: &syn::Field) -> bool {
    for attr in &field.attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        let Meta::List(list) = &attr.meta else {
            continue;
        };
        let tokens = list.tokens.to_string();
        if tokens.contains("default") {
            return true;
        }
    }
    false
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
        Kind::Int(rust_ty) => {
            let cast: TokenStream2 = rust_ty.parse().unwrap();
            quote! {
                ::tatara_lisp::domain::extract_int(&kw, #key)? as #cast
            }
        }
        Kind::OptionalInt(rust_ty) => {
            let cast: TokenStream2 = rust_ty.parse().unwrap();
            quote! {
                ::tatara_lisp::domain::extract_optional_int(&kw, #key)?.map(|n| n as #cast)
            }
        }
        Kind::Float(rust_ty) => {
            let cast: TokenStream2 = rust_ty.parse().unwrap();
            quote! {
                ::tatara_lisp::domain::extract_float(&kw, #key)? as #cast
            }
        }
        Kind::OptionalFloat(rust_ty) => {
            let cast: TokenStream2 = rust_ty.parse().unwrap();
            quote! {
                ::tatara_lisp::domain::extract_optional_float(&kw, #key)?.map(|n| n as #cast)
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
