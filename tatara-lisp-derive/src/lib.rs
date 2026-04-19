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
//!         let kw = parse_kwargs(args)?;
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
use syn::{parse_macro_input, Attribute, Data, DeriveInput, Fields, LitStr, Meta, Type};

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
    }

    let expanded = quote! {
        impl ::tatara_lisp::domain::TataraDomain for #name {
            const KEYWORD: &'static str = #keyword;

            fn compile_from_args(
                args: &[::tatara_lisp::Sexp],
            ) -> ::tatara_lisp::Result<Self> {
                let kw = ::tatara_lisp::domain::parse_kwargs(args)?;
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
        Kind::Deserialize => quote! {
            {
                let sexp = ::tatara_lisp::domain::required(&kw, #key)?;
                let json = ::tatara_lisp::domain::sexp_to_json(sexp);
                ::serde_json::from_value(json).map_err(|e| ::tatara_lisp::LispError::Compile {
                    form: #key.to_string(),
                    message: format!("deserialize: {e}"),
                })?
            }
        },
        Kind::OptionalDeserialize => quote! {
            match kw.get(#key) {
                None => None,
                Some(sexp) => {
                    let json = ::tatara_lisp::domain::sexp_to_json(sexp);
                    Some(::serde_json::from_value(json).map_err(|e| ::tatara_lisp::LispError::Compile {
                        form: #key.to_string(),
                        message: format!("deserialize: {e}"),
                    })?)
                }
            }
        },
        Kind::VecDeserialize => quote! {
            match kw.get(#key) {
                None => ::std::vec::Vec::new(),
                Some(sexp) => {
                    let list = sexp.as_list().ok_or_else(|| ::tatara_lisp::LispError::Compile {
                        form: #key.to_string(),
                        message: "expected list".into(),
                    })?;
                    list.iter().map(|item| {
                        let json = ::tatara_lisp::domain::sexp_to_json(item);
                        ::serde_json::from_value(json).map_err(|e| ::tatara_lisp::LispError::Compile {
                            form: #key.to_string(),
                            message: format!("deserialize: {e}"),
                        })
                    }).collect::<::tatara_lisp::Result<::std::vec::Vec<_>>>()?
                }
            }
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
