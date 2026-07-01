//! S-expression AST.

use crate::error::{SexpShape, SexpWitness, StructuralKind, UnquoteForm};
use std::fmt;
use std::hash::{Hash, Hasher};

// `Sexp` is `PartialEq` but not `Eq` (Float contains NaN). We implement Hash
// manually so cache keys can hash a borrowed `&[Sexp]` directly — avoids the
// serde_json serialization that would otherwise dominate cache overhead on
// cheap macro calls.
impl Hash for Sexp {
    fn hash<H: Hasher>(&self, h: &mut H) {
        match self {
            Self::Nil => 0u8.hash(h),
            Self::Atom(a) => {
                1u8.hash(h);
                a.hash(h);
            }
            Self::List(items) => {
                2u8.hash(h);
                items.len().hash(h);
                for i in items {
                    i.hash(h);
                }
            }
            // The four quote-family variants share the (discriminator, inner)
            // hash shape — all route through `as_quote_form`'s typed-marker
            // projection so the per-variant discriminator bytes (3/4/5/6) and
            // the recursive `inner.hash(h)` body bind at ONE site on the
            // closed-set `QuoteForm` algebra. The `.expect(_)` is a
            // static-invariant statement (the outer pattern guarantees the
            // projection lands `Some`) that a future quote-family extension
            // can't drift across — adding a fifth `Sexp` wrapper variant
            // forces a corresponding `QuoteForm` extension AND
            // `as_quote_form` arm, with rustc binding the three together.
            Self::Quote(_) | Self::Quasiquote(_) | Self::Unquote(_) | Self::UnquoteSplice(_) => {
                let (qf, inner) = self.expect_quote_form();
                qf.hash_discriminator().hash(h);
                inner.hash(h);
            }
        }
    }
}

// The six atomic variants share the (discriminator, inner) hash shape —
// the per-variant discriminator byte binds at ONE site on the closed-set
// `AtomKind` algebra (`AtomKind::hash_discriminator`) rather than at six
// inline `<N>u8.hash(h)` arms here. The inner-payload arm stays a match
// because the payload type differs per variant (`String` for symbol /
// keyword / str, `i64` for int, `f64::to_bits()` for float, `bool` for
// bool); the or-pattern collapses the three string-carrying arms. Float:
// hash the bit pattern. NaN != NaN so PartialEq is broken, but cache
// lookups use PartialEq-by-hash which this satisfies modulo a NaN
// collision risk we accept for template args. The (Atom variant, byte)
// pairing is pinned bit-for-bit by `atom_kind_hash_discriminator_pins_
// legacy_atom_cache_key_bytes` against the pre-lift 0/1/2/3/4/5 sequence
// — same posture as `quote_form_hash_discriminator_pins_legacy_cache_
// key_bytes` for the four-of-thirteen `Sexp` wrapper variants.
impl Hash for Atom {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.kind().hash_discriminator().hash(h);
        match self {
            Self::Symbol(s) | Self::Keyword(s) | Self::Str(s) => s.hash(h),
            Self::Int(n) => n.hash(h),
            Self::Float(f) => f.to_bits().hash(h),
            Self::Bool(b) => b.hash(h),
        }
    }
}

/// An S-expression — the homoiconic value + program representation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Sexp {
    Nil,
    Atom(Atom),
    List(Vec<Sexp>),
    /// `'x` — literal; does not participate in macro substitution.
    Quote(Box<Sexp>),
    /// `` `x `` — quasi-quotation; substitution happens inside.
    Quasiquote(Box<Sexp>),
    /// `,x` — substitute the binding named `x`. Only valid inside a quasi-quote.
    Unquote(Box<Sexp>),
    /// `,@x` — splice the list `x` into the containing list.
    UnquoteSplice(Box<Sexp>),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Atom {
    /// Plain symbol (`foo`, `defpoint`, `seph.1`).
    Symbol(String),
    /// Keyword (`:parent`, `:attr`) — a symbol bound to itself.
    Keyword(String),
    /// String literal.
    Str(String),
    /// Integer literal.
    Int(i64),
    /// Floating literal.
    Float(f64),
    /// Boolean literal (`#t`, `#f`).
    Bool(bool),
}

impl Atom {
    /// Canonical [`Self::Symbol`] constructor — first of the six per-
    /// variant typed-construct methods on the closed-set [`Atom`]
    /// algebra. Takes `impl Into<String>` so the consumer composes any
    /// `&str` / `String` / `Cow<'_, str>` into the typed payload without
    /// pre-coercing at its site — the `.into()` boundary lives at this
    /// method on the algebra, parallel to how the [`Sexp`] outer
    /// constructors ([`Sexp::symbol`], [`Sexp::keyword`],
    /// [`Sexp::string`]) accept the same `impl Into<String>` shape at
    /// the outer algebra layer.
    ///
    /// Sibling typed-construct family on the closed-set [`Atom`]
    /// algebra — paired section-for-retraction with the soft-projection
    /// family ([`Self::as_symbol`], [`Self::as_keyword`],
    /// [`Self::as_string`], [`Self::as_int`], [`Self::as_float`],
    /// [`Self::as_bool`]). Pre-lift the typed-construct family was
    /// missing from the algebra: consumers reached for the bare
    /// `Self::Symbol(s.into())` tuple-variant constructor + `.into()`
    /// coercion at every site (with no `impl Into` ergonomy on the
    /// algebra), AND the soft-projection family had no constructor
    /// peer — section-for-retraction was uneven. Post-lift every
    /// consumer that builds an [`Atom`] from a typed payload at one
    /// site AND projects an [`Atom`] back to its typed payload at
    /// another binds to ONE method per direction on the algebra. The
    /// six [`Sexp`] outer constructors ([`Sexp::symbol`] through
    /// [`Sexp::boolean`]) route through `Self::Atom(Atom::X(_))` —
    /// `.into()` ergonomy on the inner algebra is reused at the outer
    /// algebra without re-derivation.
    ///
    /// Round-trip law binding it to the soft-projection sibling: for
    /// every `s: &str`, `Atom::symbol(s).as_symbol() == Some(s)` —
    /// every other arm projects to `None`. Same posture across the
    /// five sibling pairs (`Atom::keyword(s).as_keyword() == Some(s)`,
    /// …). The `kind()` projection ([`Self::kind`]) similarly
    /// round-trips through the construct face: `Atom::symbol(_).kind()
    /// == AtomKind::Symbol`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 2 — free middle;
    /// every consumer that constructs an [`Atom`] of a typed kind binds
    /// to ONE typed method on the algebra rather than to the bare
    /// tuple-variant constructor + per-site `.into()` coercion.
    /// THEORY.md §V.1 — knowable platform; the `(AtomKind variant,
    /// typed construct method)` pair becomes a TYPE projection on the
    /// substrate's [`Atom`] algebra. THEORY.md §VI.1 — generation over
    /// composition; the `[Sexp; 6]` outer constructors at
    /// [`Sexp::symbol`]–[`Sexp::boolean`] regenerate identically
    /// through `Self::Atom(Atom::X(_))` composition rather than
    /// re-deriving the `.into()` + tuple-variant pair per outer
    /// constructor.
    ///
    /// Frontier inspiration: Racket's `(symbol 'x)` / `(string s)` —
    /// the typed-construct face the consumer reaches for a typed
    /// atomic value paired one-for-one with `(symbol? v)` /
    /// `(symbol->string v)` predicate/projection siblings; the
    /// substrate's [`Self::symbol`] / [`Self::as_symbol`] pair is the
    /// Rust-typed peer on the closed-set [`Atom`] algebra, with
    /// `impl Into<String>` standing in for Racket's typed-pair coerce
    /// face. MLIR's `mlir::SymbolAttr::get(ctx, name)` — typed-IR
    /// attribute construction routes through ONE typed factory paired
    /// with `mlir::dyn_cast<SymbolAttr>(attr)` on the projection face;
    /// `Atom::symbol` is the substrate's unstructured-Rust peer.
    #[must_use]
    pub fn symbol(s: impl Into<String>) -> Self {
        Self::Symbol(s.into())
    }

    /// Canonical [`Self::Keyword`] constructor — second of the six
    /// per-variant typed-construct methods on the closed-set [`Atom`]
    /// algebra. See [`Self::symbol`] for the algebra-level docstring.
    #[must_use]
    pub fn keyword(s: impl Into<String>) -> Self {
        Self::Keyword(s.into())
    }

    /// Canonical [`Self::Str`] constructor — third of the six per-variant
    /// typed-construct methods. The method name is `string` for
    /// consumer-vocabulary continuity with [`Self::as_string`] /
    /// [`Sexp::string`] / [`crate::error::SexpShape::String`] (the typed
    /// payload variant is `Str` for `String` shortening; the consumer-
    /// facing method keeps `string` for symmetry).
    #[must_use]
    pub fn string(s: impl Into<String>) -> Self {
        Self::Str(s.into())
    }

    /// Canonical [`Self::Int`] constructor — fourth of the six per-variant
    /// typed-construct methods. The `i64` is taken by value (no
    /// `impl Into<…>` widening) — strict typed identity at the algebra
    /// boundary, the same posture [`Self::as_int`] preserves on the
    /// soft-projection face (`Atom::Int(n)` projects to `Some(n)` only;
    /// the `Sexp::as_float` consumer is where Int→Float widening lives).
    #[must_use]
    pub fn int(n: i64) -> Self {
        Self::Int(n)
    }

    /// Canonical [`Self::Float`] constructor — fifth of the six
    /// per-variant typed-construct methods. The `f64` is taken by value
    /// (no `impl Into<…>` widening), matching [`Self::int`]'s strict
    /// typed-identity posture at the algebra boundary.
    #[must_use]
    pub fn float(n: f64) -> Self {
        Self::Float(n)
    }

    /// Canonical [`Self::Bool`] constructor — sixth and last of the six
    /// per-variant typed-construct methods on the closed-set [`Atom`]
    /// algebra. Together with the five siblings ([`Self::symbol`],
    /// [`Self::keyword`], [`Self::string`], [`Self::int`],
    /// [`Self::float`]) the per-`Atom`-variant typed-construct family is
    /// complete across all six closed-set arms, and pairs section-for-
    /// retraction with the soft-projection family ([`Self::as_symbol`],
    /// [`Self::as_keyword`], [`Self::as_string`], [`Self::as_int`],
    /// [`Self::as_float`], [`Self::as_bool`]) — every consumer that
    /// constructs an [`Atom`] from a typed payload at one site AND
    /// projects an [`Atom`] back to its typed payload at another binds
    /// to ONE method per direction on the algebra rather than to the
    /// bare tuple-variant constructor + the soft-projection method
    /// asymmetrically.
    ///
    /// The closed-set `bool` payload's Scheme-canonical `#t` / `#f`
    /// reader lexemes are dispatched at [`Self::from_lexeme`] (the
    /// typed-ENTRY classifier) — this method is the construction face
    /// the consumer composes the typed `bool` value into when building
    /// an [`Atom`] from already-typed Rust, parallel to how
    /// [`Self::int`] and [`Self::float`] take their typed payload by
    /// value.
    #[must_use]
    pub fn boolean(b: bool) -> Self {
        Self::Bool(b)
    }

    /// Project the atomic value into its closed-set [`AtomKind`] marker —
    /// `Symbol(_) → AtomKind::Symbol`, `Keyword(_) → AtomKind::Keyword`,
    /// `Str(_) → AtomKind::Str`, `Int(_) → AtomKind::Int`,
    /// `Float(_) → AtomKind::Float`, `Bool(_) → AtomKind::Bool`. The
    /// projection discards the payload and surfaces the typed
    /// discriminator that every per-atom-kind dispatch site (Hash cache-
    /// key bytes via [`AtomKind::hash_discriminator`], outer-shape
    /// projection via [`AtomKind::sexp_shape`], diagnostic label via
    /// [`AtomKind::label`]) keys on.
    ///
    /// Soft-projection peer of [`Sexp::as_quote_form`]: where
    /// `as_quote_form` decomposes the four homoiconic prefix wrappers
    /// into `(QuoteForm, &Sexp)`, `kind` decomposes the six atomic
    /// payloads into `AtomKind` alone — there is no inner-sexp body to
    /// surface, so the projection's return type is just the marker.
    /// Sibling-arm sweep with the quote-family `as_quote_form` /
    /// `QuoteForm` algebra lifts the (Atom variant, byte-discriminator,
    /// canonical-label, SexpShape variant) quadruple from per-callsite
    /// discipline (`Hash for Atom`'s six byte literals AND
    /// `domain::sexp_shape`'s six SexpShape literals) onto ONE typed
    /// algebra the substrate's diagnostic + cache-key surfaces both
    /// route through.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 2 — free middle; the
    /// (Atom variant, downstream-consumer-payload) pairing now binds at
    /// ONE typed projection site (this method composed with
    /// [`AtomKind`]'s arms) regardless of which consumer surface
    /// (cache-key Hash, diagnostic SexpShape, future LSP completion
    /// label) needs it. A regression that drifts ONE consumer's pairing
    /// from the others cannot reach the substrate's runtime.
    #[must_use]
    pub fn kind(&self) -> AtomKind {
        match self {
            Self::Symbol(_) => AtomKind::Symbol,
            Self::Keyword(_) => AtomKind::Keyword,
            Self::Str(_) => AtomKind::Str,
            Self::Int(_) => AtomKind::Int,
            Self::Float(_) => AtomKind::Float,
            Self::Bool(_) => AtomKind::Bool,
        }
    }

    /// Project the atomic payload to its canonical [`serde_json::Value`]
    /// rendering — the typed-algebra peer of [`fmt::Display for Atom`] at
    /// the JSON-projection boundary. Lifts six inline atom arms inside
    /// [`crate::domain::sexp_to_json`]'s outer match (one
    /// `Sexp::Atom(Atom::<variant>(payload)) => JValue::<…>(…)` arm
    /// per [`AtomKind`] variant) onto ONE typed-algebra method that
    /// every consumer routes through. Sibling-shape lift to the prior
    /// `Display for Atom` (the canonical-string rendering surface),
    /// `Hash for Atom` (the cache-key bytes surface via
    /// [`AtomKind::hash_discriminator`]), and the upcoming
    /// `Atom::to_iac_forge_sexpr` (the canonical-SExpr rendering
    /// surface, feature-gated `iac-forge`) — every per-`Atom`-variant
    /// projection now binds at ONE method on the closed-set algebra
    /// rather than at six inline arms inside its consumer.
    ///
    /// Mapping (preserves the byte-identical pre-lift behavior at the
    /// `sexp_to_json` callsite):
    ///   * [`Self::Symbol`] payload `s` → [`serde_json::Value::String`] of
    ///     `s` cloned (Symbols are enum discriminants — the JSON
    ///     deserializer reads them as the string-form variant tag).
    ///   * [`Self::Keyword`] payload `s` → [`serde_json::Value::String`]
    ///     of `":{s}"` (Keywords prefix with `:` in their canonical
    ///     wire-form; `json_to_sexp`'s inverse strips the prefix).
    ///   * [`Self::Str`] payload `s` → [`serde_json::Value::String`] of
    ///     `s` cloned.
    ///   * [`Self::Int`] payload `n` → [`serde_json::Value::Number`] of
    ///     `n` (lossless via `serde_json::Number::from(i64)`).
    ///   * [`Self::Float`] payload `n` → [`serde_json::Value::Number`] of
    ///     `n` IFF `n` is finite (NaN / ±∞ collapse to
    ///     [`serde_json::Value::Null`]; this is JSON's structural
    ///     inexpressibility of those f64 values, NOT a substrate
    ///     choice). The NaN/∞→Null branch is pinned at one test below
    ///     (`atom_to_json_float_nan_and_infinity_collapse_to_null`).
    ///   * [`Self::Bool`] payload `b` → [`serde_json::Value::Bool`] of
    ///     `b`.
    ///
    /// Bidirectional contract anchored by tests in this module:
    ///   * `atom_to_json_projects_each_variant_to_canonical_json_value`
    ///     — sweeps a representative atom of each [`AtomKind`] variant
    ///     and pins each variant's canonical JValue mapping
    ///     byte-for-byte against the pre-lift inline rule, so a future
    ///     regression that drifts ONE arm (e.g. swaps `Symbol`'s
    ///     mapping to a Number, or drops `Keyword`'s `:` prefix) fails
    ///     loudly.
    ///   * `atom_to_json_float_nan_and_infinity_collapse_to_null`
    ///     — pins the JSON-structural inexpressibility branch at the
    ///     atom layer directly, so a future Atom-Display-style refactor
    ///     that bypasses [`serde_json::Number::from_f64`] (e.g. tries
    ///     to emit `NaN` as the string `"NaN"`) surfaces at the
    ///     typed-algebra boundary without requiring a Sexp wrap.
    ///   * `sexp_to_json_atom_arms_route_through_atom_to_json` (in
    ///     [`crate::domain::tests`]) — pins the lifted boundary:
    ///     `sexp_to_json(&Sexp::Atom(a.clone())) == Ok(a.to_json())`
    ///     for every atomic payload variant. Catches a future drift
    ///     where one surface's per-variant body changes without the
    ///     other.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition;
    /// the (Atom variant, canonical JValue rendering) pair lived inline
    /// at the `sexp_to_json` site as six byte-identical arms. The lift
    /// retires the per-site fan-out onto ONE method on the `Atom`
    /// algebra. THEORY.md §II.1 invariant 2 — free middle; the typed-
    /// exit JSON projection, the Display-surface rendering, the
    /// diagnostic surface, and any future canonical-form surface
    /// (e.g. `Atom::to_iac_forge_sexpr`) all route through ONE
    /// per-variant projection family rather than per-callsite
    /// re-derivation. THEORY.md §V.1 — knowable platform; a future
    /// seventh atomic kind (e.g. `Char` for `#\x` reader syntax) lands
    /// at one [`AtomKind::ALL`] entry plus one arm here plus one arm
    /// per sibling projection — exhaustively checked by the compiler,
    /// not by per-consumer convention.
    ///
    /// Frontier inspiration: MLIR's `mlir::AsmPrinter::printAttribute`
    /// — the typed-IR attribute printer dispatches on the closed-set
    /// `AttributeKind` so every printer body for a kind lives at ONE
    /// implementation site; `Atom::to_json` is the unstructured-Rust
    /// peer on the `Atom` algebra for the JSON canonical-form surface
    /// (where `Display for Atom` is the Lisp-canonical-form peer and
    /// `From<&Sexp> for iac_forge::SExpr` is the canonical-attestation-
    /// form peer). Racket's `(syntax->datum stx)` then a serializer
    /// over the datum prim — `to_json` is the substrate's serializer
    /// at the atomic-payload layer, with the closed-set `AtomKind`
    /// standing in for Racket's datum-prim taxonomy.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Symbol(s) => serde_json::Value::String(s.clone()),
            Self::Keyword(s) => serde_json::Value::String(format!(":{s}")),
            Self::Str(s) => serde_json::Value::String(s.clone()),
            Self::Int(n) => serde_json::Value::Number((*n).into()),
            Self::Float(n) => serde_json::Number::from_f64(*n)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            Self::Bool(b) => serde_json::Value::Bool(*b),
        }
    }

    /// Project the atomic payload to its canonical
    /// [`iac_forge::sexpr::SExpr`] rendering — the typed-algebra peer of
    /// [`fmt::Display for Atom`] and [`Self::to_json`] at the
    /// canonical-attestation-form boundary. Lifts six inline atom arms
    /// inside [`crate::interop::iac_forge_impl::From<&Sexp> for SExpr`]'s
    /// outer match (one `Atom::<variant>(payload) => SExpr::<…>(…)` arm
    /// per [`AtomKind`] variant) onto ONE typed-algebra method that
    /// every consumer routes through. Completes the sibling-shape lift
    /// to [`fmt::Display for Atom`] (the Lisp canonical-form surface)
    /// and [`Self::to_json`] (the JSON canonical-form surface) — every
    /// per-`Atom`-variant projection across all THREE production-site
    /// rendering surfaces now binds at ONE method on the closed-set
    /// algebra rather than at six inline arms inside its consumer.
    ///
    /// Mapping (preserves the byte-identical pre-lift behavior at the
    /// interop callsite):
    ///   * [`Self::Symbol`] payload `s` → [`iac_forge::sexpr::SExpr::Symbol`]
    ///     of `s` cloned.
    ///   * [`Self::Keyword`] payload `s` → [`iac_forge::sexpr::SExpr::Symbol`]
    ///     of `":{s}"` (keywords encoded as `:name` symbols in
    ///     canonical form — same `:` prefix convention as
    ///     [`Self::to_json`]'s string-prefixed encoding, but at the
    ///     SExpr::Symbol arm rather than the JSON String value because
    ///     iac-forge's algebra has no distinct keyword variant).
    ///   * [`Self::Str`] payload `s` → [`iac_forge::sexpr::SExpr::String`]
    ///     of `s` cloned.
    ///   * [`Self::Int`] payload `n` → [`iac_forge::sexpr::SExpr::Integer`]
    ///     of `n`.
    ///   * [`Self::Float`] payload `n` → [`iac_forge::sexpr::SExpr::Float`]
    ///     of `n` (no NaN/∞ collapse — iac-forge's `SExpr::Float` carries
    ///     `f64` natively; the JSON-structural inexpressibility branch
    ///     pinned at [`Self::to_json`] does not apply here).
    ///   * [`Self::Bool`] payload `b` → [`iac_forge::sexpr::SExpr::Bool`]
    ///     of `b`.
    ///
    /// Bidirectional contract anchored by tests in the
    /// [`crate::interop`] module's `#[cfg(test)] mod tests` block:
    ///   * `atom_to_iac_forge_sexpr_projects_each_variant_to_canonical_sexpr`
    ///     — sweeps a representative atom of each [`AtomKind`] variant
    ///     and pins each variant's canonical SExpr mapping byte-for-byte
    ///     against the pre-lift inline rule, so a future regression that
    ///     drifts ONE arm (e.g. swaps `Symbol`'s mapping to a String,
    ///     drops `Keyword`'s `:` prefix that downstream BLAKE3 attestation
    ///     keys hash, or renames `Str → Integer`) fails loudly.
    ///   * `sexp_atom_iac_forge_arm_routes_through_atom_to_iac_forge_sexpr`
    ///     — pins the lifted boundary:
    ///     `SExpr::from(&Sexp::Atom(a.clone())) == a.to_iac_forge_sexpr()`
    ///     for every atomic payload variant. Catches a future drift
    ///     where the outer `From<&Sexp>` arm re-inlines ONE variant's
    ///     rendering without updating the typed projection.
    ///
    /// Feature-gated on `iac-forge` mirroring the impl in
    /// [`crate::interop::iac_forge_impl`] — the method's return type
    /// references [`iac_forge::sexpr::SExpr`], so the projection only
    /// exists when the consumer crate compiled the optional dependency
    /// in. Sibling-feature posture to the substrate's
    /// `#[cfg(feature = "iac-forge")]`-gated `From<&Sexp> for SExpr`
    /// impl.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition;
    /// the (Atom variant, canonical SExpr rendering) pair lived inline
    /// at the interop site as six byte-identical arms. The lift retires
    /// the per-site fan-out onto ONE method on the `Atom` algebra,
    /// completing the three-surface sweep ([`fmt::Display for Atom`],
    /// [`Self::to_json`], [`Self::to_iac_forge_sexpr`]) the prior runs
    /// in this series named. THEORY.md §II.1 invariant 2 — free middle;
    /// the typed-exit Display rendering, the JSON projection, the
    /// canonical-attestation-form projection, the diagnostic surface,
    /// and the cache-key hash surface ALL route through ONE
    /// per-variant projection family rather than per-callsite
    /// re-derivation. THEORY.md §V.1 — knowable platform; a future
    /// seventh atomic kind (e.g. `Char` for `#\x` reader syntax) lands
    /// at one [`AtomKind::ALL`] entry plus one arm per projection —
    /// exhaustively checked by the compiler across every consumer
    /// surface, not by per-consumer convention.
    ///
    /// Frontier inspiration: MLIR's `mlir::AsmPrinter::printAttribute`
    /// dispatches on the closed-set `AttributeKind` so every printer
    /// body for a kind lives at ONE implementation site;
    /// `Atom::to_iac_forge_sexpr` is the unstructured-Rust peer on the
    /// `Atom` algebra for the canonical-attestation-form surface (the
    /// THIRD and LAST of the three production-site atom-arm shapes
    /// after `Display for Atom` and `Atom::to_json`). Racket's
    /// `(syntax->datum stx)` then a serializer over the datum prim —
    /// `to_iac_forge_sexpr` is the substrate's serializer at the
    /// atomic-payload layer for the cross-crate attestation algebra,
    /// with the closed-set `AtomKind` standing in for Racket's
    /// datum-prim taxonomy.
    #[cfg(feature = "iac-forge")]
    #[must_use]
    pub fn to_iac_forge_sexpr(&self) -> iac_forge::sexpr::SExpr {
        use iac_forge::sexpr::SExpr;
        match self {
            Self::Symbol(s) => SExpr::Symbol(s.clone()),
            // Keywords encoded as `:name` symbols in canonical form —
            // same `:` prefix convention as `Atom::to_json`'s
            // string-prefixed encoding.
            Self::Keyword(s) => SExpr::Symbol(format!(":{s}")),
            Self::Str(s) => SExpr::String(s.clone()),
            Self::Int(n) => SExpr::Integer(*n),
            Self::Float(n) => SExpr::Float(*n),
            Self::Bool(b) => SExpr::Bool(*b),
        }
    }

    /// Classify a bare reader-token lexeme into its typed [`Atom`]
    /// variant — the typed-ENTRY mirror of the three typed-EXIT
    /// projections on the [`Atom`] algebra ([`fmt::Display for Atom`],
    /// [`Self::to_json`], [`Self::to_iac_forge_sexpr`]). Lifts the
    /// five-statement classification cascade that lived inline at the
    /// reader's private `atom_from_str` helper onto ONE typed-algebra
    /// method on the closed-set [`Atom`] algebra; the reader's
    /// `Token::Atom(s)` arm collapses to `Sexp::Atom(Atom::from_lexeme(&s))`.
    /// Completes the bidirectional sweep across the four production-site
    /// per-`Atom`-variant projection shapes (typed-exit Display, JSON,
    /// iac-forge canonical attestation, AND now typed-entry
    /// classification) onto the algebra.
    ///
    /// Classification rule (byte-identical to the pre-lift reader
    /// `atom_from_str` cascade):
    ///   1. `"#t"`/`"#f"` → [`Self::Bool`] — the Scheme bool spellings;
    ///      bare `true`/`false` re-read as [`Self::Symbol`] (the
    ///      CLAUDE.md "Lisp bools" warning — every `:values-overlay`
    ///      payload depends on this for `Value::Bool` round-trip).
    ///   2. `:foo` (leading `:`) → [`Self::Keyword`] — strips the `:`
    ///      so the inverse [`fmt::Display`] rule (`Keyword(s) →
    ///      ":{s}"`) round-trips.
    ///   3. `i64::from_str` succeeds → [`Self::Int`] — load-bearing
    ///      ORDERING: tried BEFORE `f64` so `"1"` classifies as
    ///      [`Self::Int`]`(1)`, NOT [`Self::Float`]`(1.0)`. Typed-int-
    ///      vs-typed-float distinction at the Display→read boundary
    ///      is the dual of `fmt_float`'s `.0`-suffix discipline.
    ///   4. `f64::from_str` succeeds → [`Self::Float`].
    ///   5. Default → [`Self::Symbol`].
    ///
    /// Composition laws (pinned by tests below):
    ///   * `Atom::from_lexeme(&a.to_string()) == a` for every variant
    ///     EXCEPT [`Self::Str`] (Display renders Str with quote marks
    ///     — strings take the reader's `"`-quoted tokenizer branch,
    ///     NOT the bare-atom branch).
    ///   * `read(s)` for every canonical bare-atom source lexeme
    ///     equals `vec![Sexp::Atom(Atom::from_lexeme(s))]` (pinned by
    ///     `reader_atom_token_arm_routes_through_atom_from_lexeme_for_
    ///     every_kind` in [`crate::reader::tests`]).
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 1 — typed entry;
    /// `atom_from_str` was the typed-entry gate as a free function in
    /// `reader.rs`, outside the typed `Atom` algebra. Naming it on the
    /// algebra brings the typed-entry side INTO the same closed-set
    /// match family the typed-exit projections live on, so a future
    /// seventh atomic kind (e.g. `Char` for `#\x` reader syntax) lands
    /// at ONE [`AtomKind::ALL`] entry plus ONE arm here plus ONE arm
    /// per typed-exit projection — exhaustively checked by rustc
    /// across all FOUR per-variant projection families. THEORY.md
    /// §II.1 invariant 2 — free middle; FOUR consumers (typed-entry
    /// classification, Display rendering, JSON projection, canonical-
    /// attestation-form projection) now route through ONE
    /// per-`Atom`-variant projection family on the closed-set algebra.
    /// THEORY.md §VI.1 — generation over composition; this lift
    /// completes the bidirectional sweep across the four production
    /// surfaces the prior runs in this series named.
    ///
    /// Frontier inspiration: Racket's `(read-syntax …)` dispatches a
    /// bare-atom lexeme through a closed-set classifier keyed on
    /// prefix + parse-as-numeric cascade; `Atom::from_lexeme` is the
    /// substrate's typed-Rust peer, with [`AtomKind`] standing in for
    /// Racket's datum-prim taxonomy. MLIR's
    /// `mlir::AsmParser::parseAttribute` dispatches on the closed-set
    /// `AttributeKind` so every parser body for a kind lives at ONE
    /// implementation site; `Atom::from_lexeme` is the
    /// unstructured-Rust peer on the [`Atom`] algebra for the
    /// typed-entry classification surface.
    #[must_use]
    pub fn from_lexeme(s: &str) -> Self {
        if s == "#t" {
            return Self::Bool(true);
        }
        if s == "#f" {
            return Self::Bool(false);
        }
        if let Some(rest) = s.strip_prefix(':') {
            return Self::Keyword(rest.to_owned());
        }
        if let Ok(n) = s.parse::<i64>() {
            return Self::Int(n);
        }
        if let Ok(n) = s.parse::<f64>() {
            return Self::Float(n);
        }
        Self::Symbol(s.to_owned())
    }

    /// Soft projection onto the [`Self::Symbol`] payload — `Some(&str)`
    /// iff this is a [`Self::Symbol`] variant, `None` for every other
    /// atomic kind (`Keyword`, `Str`, `Int`, `Float`, `Bool`).
    ///
    /// FIRST of the six per-variant soft-projection methods on the typed
    /// [`Atom`] algebra — the typed-EXIT *soft*-projection peer of the
    /// typed-EXIT canonical-form projections ([`fmt::Display for Atom`],
    /// [`Self::to_json`], [`Self::to_iac_forge_sexpr`]) and the typed-ENTRY
    /// classifier ([`Self::from_lexeme`]). Where the canonical-form trio
    /// projects the atomic payload to a *rendered* canonical surface
    /// (string / JSON / iac-forge SExpr) and the classifier projects a
    /// lexeme to the typed `Atom`, this method projects the typed `Atom`
    /// to its inner payload — the soft-decomposition face of the closed
    /// set, completing the algebra surface across BOTH bidirectional axes
    /// (canonical-form rendering + classification on the typed-ENTRY/
    /// typed-EXIT axis; soft decomposition on the typed-EXIT side at the
    /// payload axis).
    ///
    /// Sibling soft-projection peer of [`Sexp::as_quote_form`]: where
    /// `as_quote_form` soft-decomposes the four homoiconic prefix
    /// wrappers into `Option<(QuoteForm, &Sexp)>`, this method (and its
    /// five `as_*` siblings on [`Atom`]) soft-decompose the six atomic
    /// payloads into `Option<&str>` / `Option<i64>` / `Option<f64>` /
    /// `Option<bool>` — there is no inner-sexp body to surface, so the
    /// projection's return type is just the payload. The
    /// `Sexp::as_symbol` consumer at the `Sexp` algebra layer composes
    /// this projection with [`Sexp::as_atom`] (the structural lift to
    /// the inner [`Atom`]) — `Sexp::as_symbol(self) ==
    /// self.as_atom().and_then(Atom::as_symbol)` — so the per-`Atom`-
    /// variant soft-projection binds at ONE method on the typed algebra
    /// rather than at six inline `Self::Atom(Atom::X(s)) => Some(s)` arms
    /// inside the `Sexp` consumer.
    ///
    /// Lifts the inline `Self::Atom(Atom::Symbol(s)) => Some(s)` arm at
    /// [`Sexp::as_symbol`]'s match body onto ONE typed-algebra projection
    /// the `Sexp` consumer routes through via the structural lift
    /// [`Sexp::as_atom`]. Sibling-shape lift to the typed-EXIT
    /// canonical-form projections (`Display for Atom`, `Atom::to_json`,
    /// `Atom::to_iac_forge_sexpr`) and the typed-ENTRY classifier
    /// (`Atom::from_lexeme`) — every per-`Atom`-variant projection
    /// across both the rendering surfaces AND the soft-decomposition
    /// surface now binds at ONE method on the closed-set algebra rather
    /// than at inline arms inside its consumer.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 2 — free middle; the
    /// (Atom variant, downstream-consumer-payload) pairing now binds at
    /// ONE typed projection per consumer surface (six canonical-form
    /// surfaces — `Display`, JSON, iac-forge, plus the soft-projection
    /// FAMILY this method opens), regardless of which consumer reaches
    /// in. THEORY.md §VI.1 — generation over composition; the six inline
    /// `Self::Atom(Atom::X(s)) => Some(_)` arms at `Sexp::as_X` sites
    /// (well past the ≥2 PRIME-DIRECTIVE trigger once the structural
    /// shape is named) collapse onto the closed-set `Atom` algebra so a
    /// future seventh atomic kind (e.g. `Char` for `#\x` reader syntax,
    /// `Bigint` for arbitrary-precision integers) extends `Atom::ALL` +
    /// the per-variant soft-projection method ONCE and rustc enforces
    /// matching across every consumer through the closed-set match.
    /// THEORY.md §V.1 — knowable platform; the (Atom variant, payload)
    /// pairing becomes a TYPE projection on the substrate algebra
    /// rather than six inline arms at the `Sexp` consumer. A typo or
    /// swap at the soft-projection site is no longer a runtime drift
    /// but a compile error against the typed projection.
    ///
    /// Frontier inspiration: Racket's `(symbol? v)` / `(symbol->string
    /// v)` pair — the typed-predicate + typed-projection pair at the
    /// atomic-payload layer; this method (and its five `as_*` siblings)
    /// is the substrate's typed soft-projection peer on the closed-set
    /// `Atom` algebra, with `Option<&str>` standing in for the
    /// predicate-AND-projection pair Racket carries as two functions.
    /// MLIR's `mlir::dyn_cast<SymbolAttribute>(attr)` — the typed-IR
    /// soft-downcast onto a closed-set attribute family; `Atom::as_symbol`
    /// is the unstructured-Rust peer on the `Atom` algebra for the
    /// soft-projection face, with the closed-set `AtomKind` standing in
    /// for MLIR's `AttributeKind` taxonomy.
    #[must_use]
    pub fn as_symbol(&self) -> Option<&str> {
        match self {
            Self::Symbol(s) => Some(s),
            _ => None,
        }
    }

    /// Soft projection onto the [`Self::Keyword`] payload — `Some(&str)`
    /// iff this is a [`Self::Keyword`] variant, `None` for every other
    /// atomic kind. The returned `&str` is the payload AFTER the `:`
    /// prefix has been stripped at the typed-ENTRY classifier
    /// boundary ([`Self::from_lexeme`] strips `:` when constructing a
    /// `Keyword`; this projection surfaces the bare identifier).
    /// SECOND of the six per-variant soft-projection methods on the
    /// typed [`Atom`] algebra — see [`Self::as_symbol`] for the
    /// algebra-level docstring.
    #[must_use]
    pub fn as_keyword(&self) -> Option<&str> {
        match self {
            Self::Keyword(s) => Some(s),
            _ => None,
        }
    }

    /// Soft projection onto the [`Self::Str`] payload — `Some(&str)` iff
    /// this is a [`Self::Str`] variant (the typed `"…"`-quoted string
    /// literal payload at the reader's [`crate::reader::Token::Str`]
    /// branch), `None` for every other atomic kind. THIRD of the six
    /// per-variant soft-projection methods — named `as_string` at the
    /// `Sexp` consumer for consumer-vocabulary continuity with the
    /// pre-lift `Sexp::as_string` projection (the typed payload variant
    /// is `Str` for `String` shortening; the consumer-facing method
    /// keeps `string` for symmetry with the `ExpectedKwargShape::String`
    /// label and the [`SexpShape::String`] outer-shape marker).
    #[must_use]
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Soft projection onto the [`Self::Int`] payload — `Some(i64)` iff
    /// this is a [`Self::Int`] variant, `None` for every other atomic
    /// kind. FOURTH of the six per-variant soft-projection methods.
    /// The `i64` is returned by value (the payload is `Copy`); contrast
    /// with [`Self::as_symbol`] / [`Self::as_keyword`] / [`Self::as_string`]
    /// which borrow the underlying `String` payload as `&str` because
    /// `String` is not `Copy`.
    ///
    /// Strict typed identity: this method projects `Atom::Int(n)` to
    /// `Some(n)` only. The `Sexp::as_float` consumer at the `Sexp`
    /// algebra layer widens `Int` to `Float` (`Atom::Int(n) → Some(n as
    /// f64)`) for caller convenience at the numeric-kwarg boundary; the
    /// `Atom`-level projection here stays strict so the typed-identity
    /// distinction `Int(1)` vs `Float(1.0)` (the load-bearing typed
    /// identity at the [`Self::from_lexeme`] ⇄ Display round-trip
    /// boundary, dual of [`fmt_float`]'s `.0`-suffix discipline) is
    /// preserved at the algebra layer. The widening lives at the
    /// `Sexp::as_float` consumer (`a.as_float().or_else(|| a.as_int()
    /// .map(|n| n as f64))`) where the convenience is wanted, not at
    /// the algebra-level projection where the typed identity is
    /// load-bearing.
    #[must_use]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(n) => Some(*n),
            _ => None,
        }
    }

    /// Soft projection onto the [`Self::Float`] payload — `Some(f64)`
    /// iff this is a [`Self::Float`] variant, `None` for every other
    /// atomic kind. FIFTH of the six per-variant soft-projection
    /// methods.
    ///
    /// Strict typed identity: `Atom::Int(n)` does NOT project through
    /// this method (it stays `None`). The [`Sexp::as_float`] consumer
    /// widens `Int` to `Float` at the `Sexp` algebra layer for caller
    /// convenience; this algebra-level projection stays strict. See
    /// [`Self::as_int`]'s docstring for the typed-identity contract.
    #[must_use]
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(n) => Some(*n),
            _ => None,
        }
    }

    /// Soft projection onto the [`Self::Bool`] payload — `Some(bool)`
    /// iff this is a [`Self::Bool`] variant, `None` for every other
    /// atomic kind. SIXTH and LAST of the six per-variant soft-projection
    /// methods on the typed [`Atom`] algebra; together with the five
    /// siblings ([`Self::as_symbol`], [`Self::as_keyword`],
    /// [`Self::as_string`], [`Self::as_int`], [`Self::as_float`]) the
    /// per-`Atom`-variant soft-projection family is complete across all
    /// six closed-set arms. The CLAUDE.md-pinned `"#t"` / `"#f"` Scheme
    /// bool spellings the reader's typed-ENTRY classifier
    /// [`Self::from_lexeme`] dispatches on bind the lexeme → typed
    /// [`Self::Bool`] direction; this method binds the typed
    /// [`Self::Bool`] → payload direction at the soft-decomposition
    /// face.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Soft projection onto the *symbol-or-string* union — `Some(&str)` iff
    /// this is a [`Self::Symbol`] variant OR a [`Self::Str`] variant, `None`
    /// for every other atomic kind (`Keyword`, `Int`, `Float`, `Bool`).
    /// The atomic-payload peer of [`Sexp::as_symbol_or_string`] —
    /// disjunctive composition of [`Self::as_symbol`] + [`Self::as_string`]
    /// at the typed [`Atom`] algebra rather than at the [`Sexp`] consumer
    /// layer where the union previously composed two distinct
    /// [`Sexp::as_atom`] traversals.
    ///
    /// Sibling soft-projection peer of the six per-variant projections
    /// ([`Self::as_symbol`], [`Self::as_keyword`], [`Self::as_string`],
    /// [`Self::as_int`], [`Self::as_float`], [`Self::as_bool`]) — this
    /// union projection completes the soft-decomposition family on the
    /// closed-set [`Atom`] algebra by naming the (Symbol ⊎ Str) union
    /// the substrate's named-form NAME gate ([`crate::compile::split_name_slot`]
    /// via [`Sexp::as_symbol_or_string`]) keys on. Both NAME-author
    /// surfaces (`(defcompiler my-name …)` — bare symbol; `(defcompiler
    /// "my-name" …)` — quoted string) project to `Some("my-name")`
    /// through one method on the algebra.
    ///
    /// Composition law binding it to [`Sexp::as_symbol_or_string`]: for
    /// every [`Sexp`] `s`,
    /// `s.as_symbol_or_string() == s.as_atom().and_then(Atom::as_symbol_or_string)`
    /// — the same structural-lift composition pattern [`Sexp::as_symbol`]
    /// / [`Sexp::as_keyword`] / [`Sexp::as_string`] / [`Sexp::as_int`] /
    /// [`Sexp::as_bool`] route through on the six per-variant axis.
    /// Lifts the `self.as_symbol().or_else(|| self.as_string())`
    /// disjunctive composition at [`Sexp::as_symbol_or_string`]'s body
    /// (TWO `Sexp::as_atom` traversals pre-lift) onto ONE typed-algebra
    /// projection the `Sexp` consumer routes through via the structural
    /// lift [`Sexp::as_atom`] (ONE `Sexp::as_atom` traversal post-lift).
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 2 — free middle; the
    /// (Symbol ⊎ Str) union projection now binds at ONE method on the
    /// closed-set [`Atom`] algebra regardless of which consumer reaches
    /// in. THEORY.md §VI.1 — generation over composition; the
    /// disjunctive `as_symbol().or_else(|| as_string())` composition at
    /// [`Sexp::as_symbol_or_string`]'s body collapses onto a SINGLE
    /// structural lift through [`Sexp::as_atom`] + the algebra-level
    /// union projection, eliminating the double-traversal redundancy
    /// the pre-lift consumer-layer composition carried. THEORY.md §V.1
    /// — knowable platform; the (Symbol-or-Str) NAME-slot union becomes
    /// a TYPE projection on the substrate algebra rather than a
    /// disjunctive composition at every NAME-gate consumer.
    ///
    /// Frontier inspiration: Racket's `(or/c symbol? string?)`
    /// contract — a typed disjunctive predicate the consumer binds to
    /// in one place rather than re-deriving the disjunction at every
    /// callsite; [`Self::as_symbol_or_string`] is the substrate's
    /// unstructured-Rust peer with the typed projection (`Option<&str>`)
    /// surfacing the underlying payload alongside the predicate face.
    /// MLIR's `mlir::dyn_cast<StringLike>(attr)` — typed soft-downcast
    /// onto a closed-set attribute union; [`Self::as_symbol_or_string`]
    /// is the substrate's [`Atom`]-algebra peer for the
    /// (Symbol ⊎ Str) union, with `Option<&str>` standing in for MLIR's
    /// typed downcast result.
    #[must_use]
    pub fn as_symbol_or_string(&self) -> Option<&str> {
        self.as_symbol().or_else(|| self.as_string())
    }
}

/// Closed-set typed discriminator for the six [`Atom`] payload variants —
/// `Symbol(String)`, `Keyword(String)`, `Str(String)`, `Int(i64)`,
/// `Float(f64)`, `Bool(bool)` — paired with the projections every
/// per-atom-kind consumer keys on ([`Self::hash_discriminator`] for
/// [`Hash for Atom`]'s cache-key bytes, [`Self::sexp_shape`] for
/// [`crate::domain::sexp_shape`]'s atom-arm collapse, [`Self::label`]
/// for the operator-facing diagnostic vocabulary, [`Self::FromStr`]
/// for the typed-inverse decode that lets LSP / REPL / metric-aggregator
/// consumers round-trip a rendered diagnostic label back into the typed
/// discriminator).
///
/// Atomic-payload peer of [`QuoteForm`] (the four homoiconic prefix
/// wrappers — `Sexp::{Quote, Quasiquote, Unquote, UnquoteSplice}`):
/// where `QuoteForm` carves the closed set on `Sexp`'s wrapper-variant
/// axis, `AtomKind` carves the closed set on `Sexp`'s atomic-payload
/// axis. Together the two closed-set discriminators cover every reachable
/// `Sexp` outermost shape except `Nil` and `List` (the structural
/// constructors `()` and `(…)`) — every other shape is either an
/// `Atom(_)` projecting through this enum's [`Self::sexp_shape`] arm or a
/// quote-family wrapper projecting through [`QuoteForm::sexp_shape`].
/// After this lift the two enums' [`Self::sexp_shape`] arms own ALL TEN
/// of [`SexpShape`]'s twelve canonical labels through ONE typed
/// composition each rather than through per-callsite arm-pairing in
/// [`crate::domain::sexp_shape`].
///
/// Mirror at the atomic-payload boundary of the prior-run [`QuoteForm`]
/// (homoiconic-prefix-wrapper closed set, 4 variants), the cross-crate
/// `tatara-process` closed-set family
/// (`ConditionKind::ALL`, `ProcessPhase::ALL`, `ProcessSignal::ALL`,
/// `ChannelKind::ALL`, `IntentKind::ALL`, `LifetimeKind::ALL`,
/// `RequestorKind::ALL`, `ReceiptKind::ALL`, …) and this crate's own
/// [`SexpShape`] (the twelve reachable Sexp outermost shapes — the
/// SUPERSET this enum projects into via [`Self::sexp_shape`]) and
/// [`UnquoteForm`] (the two template-substitution markers) closed-set
/// lifts: those enums key their respective rejection or projection
/// variants on a typed identity carried inside the variant's data shape;
/// this enum keys the SIX [`Atom`] payload variants on a typed
/// discriminator identity threaded through ALL THREE per-atom-kind
/// dispatch sites ([`Hash for Atom`]'s six byte literals,
/// [`crate::domain::sexp_shape`]'s six atom arms, AND the
/// diagnostic-label vocabulary [`SexpShape::label`] publishes for the
/// atom subset). Adding a hypothetical seventh atomic kind (e.g. a
/// `Char` literal for `#\x` reader syntax, a `Bigint` for arbitrary-
/// precision integers, a `Symbol2` for namespaced symbols) requires
/// extending this enum, which rustc-enforces matching at every
/// projection site ([`Self::label`], [`Self::hash_discriminator`],
/// [`Self::sexp_shape`], [`Atom::kind`], the [`Hash for Atom`] inner
/// match, and the [`Self::FromStr`] sweep keyed on [`Self::ALL`]) — the
/// closed set becomes a TYPE rather than six `&'static str` / `u8`
/// / `SexpShape` literals that could drift independently across the
/// substrate's three per-atom-kind consumer surfaces.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 — typed entry; the
/// atomic-payload discriminator at a typed-entry rejection IS part of
/// the proof of WHAT the gate observed, and naming its closed-set
/// identity lifts the discriminator from per-site literal-pair
/// discipline (a byte at the Hash site, a SexpShape variant at the
/// `sexp_shape` site, a `&'static str` at any future LSP completion
/// site) to ONE typed enum the substrate's diagnostic + cache-key
/// surfaces both bind against. THEORY.md §II.1 invariant 2 — free
/// middle; THREE consumers ([`Hash for Atom`],
/// [`crate::domain::sexp_shape`], and the future diagnostic /
/// completion surface) route through ONE typed closed-set match
/// family, so a regression that drifts ONE consumer's pairing from the
/// others cannot reach the substrate's runtime. THEORY.md §V.1 —
/// knowable platform; the closed set of atomic payload kinds becomes a
/// TYPE rather than six byte literals (Hash) + six SexpShape literals
/// (`sexp_shape`) scattered across distinct files — a typo in any one
/// site is no longer a runtime drift but a compile error against the
/// typed projection. THEORY.md §VI.1 — generation over composition;
/// the (Atom variant, label, discriminator-byte, SexpShape variant)
/// quadruple appeared inline at THREE sites (`Hash for Atom`'s six
/// byte arms, `domain::sexp_shape`'s six atom arms, plus implicit
/// pairing across `SexpShape::label`'s six atom-subset arms) — well
/// past the ≥2 PRIME-DIRECTIVE trigger once the structural shape is
/// named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, tatara_lisp_derive::ClosedSet)]
#[closed_set(via = "label", display, generate_unknown = "atom kind")]
pub enum AtomKind {
    /// `Atom::Symbol(_)` — `"symbol"` diagnostic label, byte `0u8`
    /// hash discriminator, projects to [`SexpShape::Symbol`].
    Symbol,
    /// `Atom::Keyword(_)` — `"keyword"` diagnostic label, byte `1u8`
    /// hash discriminator, projects to [`SexpShape::Keyword`].
    Keyword,
    /// `Atom::Str(_)` — `"string"` diagnostic label, byte `2u8` hash
    /// discriminator, projects to [`SexpShape::String`].
    Str,
    /// `Atom::Int(_)` — `"int"` diagnostic label, byte `3u8` hash
    /// discriminator, projects to [`SexpShape::Int`].
    Int,
    /// `Atom::Float(_)` — `"float"` diagnostic label, byte `4u8` hash
    /// discriminator, projects to [`SexpShape::Float`].
    Float,
    /// `Atom::Bool(_)` — `"bool"` diagnostic label, byte `5u8` hash
    /// discriminator, projects to [`SexpShape::Bool`].
    Bool,
}

impl AtomKind {
    /// The closed set of six atomic [`Atom`] payload kinds — single
    /// source of truth that drives every per-kind projection
    /// ([`Self::label`] / [`fmt::Display`], [`Self::hash_discriminator`],
    /// [`Self::sexp_shape`], and the [`Self::FromStr`] decode sweep
    /// keyed on [`Self::label`]).
    ///
    /// Adding a hypothetical seventh atomic kind (e.g. `Char` for
    /// `#\x` reader syntax, `Bigint` for arbitrary-precision
    /// integers) lands at one [`Self::ALL`] entry plus one arm per
    /// projection — exhaustively checked by the compiler (the
    /// `[Self; 6]` array literal forces the arity) AND by the
    /// per-variant truth-table tests below.
    ///
    /// Sibling closed-set lift to every other typed-shape enum the
    /// substrate carries: this crate's own [`SexpShape::ALL`] (the
    /// twelve reachable outer shapes — superset of this kind's six),
    /// [`QuoteForm`] (the four homoiconic prefix wrappers — peer
    /// projection on the SAME `Sexp` algebra), [`UnquoteForm`] (the
    /// two template-substitution markers — proper subset of
    /// `QuoteForm`), and the cross-crate `tatara-process` family
    /// (`ConditionKind::ALL`, `ProcessPhase::ALL`,
    /// `ProcessSignal::ALL`, `ChannelKind::ALL`, `IntentKind::ALL`,
    /// …) every one of which paired its typed projection with `ALL`
    /// before this lift.
    ///
    /// Future consumers that compose against `ALL`: LSP / REPL
    /// completion for the operator-facing rendered atom-kind label
    /// (every `expected X, got Y` substring in `LispError`'s rendered
    /// diagnostics for an atomic witness keys on this set's projection
    /// through [`Self::label`]); `tatara-check` coverage assertions
    /// over which atomic kinds reach a `TypeMismatch.got` arm at all
    /// — the typed sweep replaces a per-callsite vocabulary of six
    /// `&'static str` literals; any future audit-trail metric jointly
    /// labeled by [`Self::label`] (e.g.
    /// `tatara_lisp_atom_type_mismatch_total{got="symbol"}`) — the
    /// metric label set IS [`Self::ALL`] mapped through
    /// [`Self::label`]; any future structural rewriter (typed
    /// analogue of MLIR's `op.walk<AtomKind::Symbol>()`) that wants
    /// to sweep over every atomic kind in a typed sequence.
    pub const ALL: [Self; 6] = [
        Self::Symbol,
        Self::Keyword,
        Self::Str,
        Self::Int,
        Self::Float,
        Self::Bool,
    ];

    /// Project the typed marker to the canonical `&'static str`
    /// diagnostic label — `"symbol"` for [`Self::Symbol`],
    /// `"keyword"` for [`Self::Keyword`], `"string"` for [`Self::Str`]
    /// (the wire-shape rename `Str → "string"` matches the
    /// [`SexpShape::String`] label projection), `"int"` for
    /// [`Self::Int`], `"float"` for [`Self::Float`], `"bool"` for
    /// [`Self::Bool`]. Each label is byte-for-byte identical to the
    /// corresponding [`SexpShape`] variant's label — and post-lift this
    /// agreement is STRUCTURAL rather than two literal-discipline sites
    /// pinned by a cross-projection test.
    ///
    /// Composition law: `AtomKind::label(k) ==
    /// AtomKind::sexp_shape(k).label()` for every `k: AtomKind`. The
    /// body composes [`Self::sexp_shape`] (the typed projection lifting
    /// each AtomKind variant into its peer [`SexpShape`] variant) with
    /// [`SexpShape::label`] (the canonical `&'static str` projection on
    /// the supeset's twelve-variant closed set), so the six atomic-arm
    /// labels live at ONE canonical site ([`SexpShape::label`]) rather
    /// than at TWO ([`SexpShape::label`] AND a parallel six-arm match
    /// here, pre-lift). Pre-lift the substrate-wide AtomKind ⊂ SexpShape
    /// label-vocabulary agreement was enforced by literal discipline at
    /// the two sites + a cross-projection test
    /// (`atom_kind_label_agrees_with_sexp_shape_label_for_every_atom_arm`);
    /// post-lift the agreement is a TYPED CONSEQUENCE of the composition
    /// — a typo in `SexpShape::label`'s atomic arms is a typo in BOTH
    /// projections, and the cross-projection test is true by
    /// construction. Same lift posture as the prior-run
    /// `Atom::as_X → Atom::as_X` algebra-lift commit (6935416), the
    /// `from_lexeme` reader-atom lift commit (9b95e64), and the
    /// `to_iac_forge_sexpr` Atom-arm lift commit (418be51): the typed
    /// projection sits on the value, and the consumer composes through
    /// the existing structural pairing rather than re-deriving the
    /// per-variant literal.
    ///
    /// The `&'static str` lifetime is load-bearing: it lets the
    /// variant project through this method without an allocation,
    /// parallel to how [`SexpShape::label`], [`QuoteForm::prefix`],
    /// [`QuoteForm::iac_forge_tag`], [`UnquoteForm::marker`], and
    /// [`crate::error::ExpectedKwargShape::label`] project their
    /// respective closed-set surfaces. The composition preserves the
    /// no-allocation contract: [`Self::sexp_shape`] returns a `Copy`
    /// value and [`SexpShape::label`] yields `&'static str`, so the
    /// `&'static str` projection through the composition allocates
    /// nothing at runtime.
    ///
    /// The bidirectional contract is anchored by tests:
    /// `atom_kind_label_renders_canonical_string_for_every_variant`
    /// pins each variant's canonical literal so a typo in
    /// [`SexpShape::label`]'s atomic arms fails-loudly through this
    /// projection too, `atom_kind_display_matches_label_for_every_variant`
    /// pins Display-equals-label so any future
    /// `#[error("... got {got}")]` annotation that threads through
    /// this projection projects byte-for-byte, and
    /// `atom_kind_label_round_trips_through_from_str` pins the
    /// `label` ↔ [`Self::FromStr`] round-trip for every variant in
    /// [`Self::ALL`] so the typed surface and the rendered diagnostic
    /// literal cannot drift. The post-lift composition contract is
    /// pinned by
    /// `atom_kind_label_routes_through_sexp_shape_label_via_sexp_shape_projection`
    /// — a regression that re-inlines the six atomic-arm literals here
    /// and silently drifts ONE arm from the [`SexpShape::label`] axis
    /// fails the routing pin loudly without needing a per-variant
    /// cross-axis literal sweep.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the
    /// AtomKind ⊂ SexpShape label-vocabulary containment becomes a
    /// TYPED CONSEQUENCE of the [`Self::sexp_shape`] + [`SexpShape::label`]
    /// composition rather than literal discipline at two sites. THEORY.md
    /// §VI.1 — generation over composition; the six atomic-arm labels
    /// live at ONE canonical site ([`SexpShape::label`]) and this method
    /// generates its identity through the typed-projection composition.
    /// THEORY.md §II.1 invariant 2 — free middle; FOUR consumers of the
    /// [`AtomKind`] algebra ([`Hash for Atom`] via
    /// [`Self::hash_discriminator`], [`crate::domain::sexp_shape`] via
    /// [`Self::sexp_shape`], the diagnostic-rendering surface via this
    /// method, and the `ClosedSet`-trait FromStr/Display surface via
    /// `#[closed_set(via = "label")]`) now route through ONE typed
    /// closed-set projection family with no per-consumer literal
    /// duplication.
    #[must_use]
    pub fn label(self) -> &'static str {
        self.sexp_shape().label()
    }

    /// Stable, per-variant byte discriminator that paired with the
    /// recursive payload hash builds the substrate's [`Hash for Atom`]
    /// projection — `0u8` for [`Self::Symbol`], `1u8` for
    /// [`Self::Keyword`], `2u8` for [`Self::Str`], `3u8` for
    /// [`Self::Int`], `4u8` for [`Self::Float`], `5u8` for
    /// [`Self::Bool`]. The byte values are load-bearing because the
    /// macro-expansion cache ([`crate::macro_expand::Expander`]'s
    /// cache) keys on the hash of `(macro_name, args)`, and any
    /// `Atom` participates in that hash — changing a discriminator
    /// silently invalidates every cached expansion across the
    /// substrate.
    ///
    /// The closed set ensures the six arms partition `{0, 1, 2, 3,
    /// 4, 5}` injectively. Disjointness from [`QuoteForm`]'s
    /// `{3, 4, 5, 6}` is structural rather than overlap-induced
    /// hash collision: [`Hash for Atom`] and the quote-family arms of
    /// [`Hash for Sexp`] hash DISTINCT types (`Atom` vs `Sexp`), and
    /// `Atom`'s discriminator lives nested INSIDE `Sexp::Atom`'s outer
    /// `1u8` discriminator — the prefix-uniqueness contract that the
    /// `Hash for Sexp` outer match maintains independently. A future
    /// quote-family or atomic-kind extension must extend BOTH bodies'
    /// arms in lockstep, with rustc binding the consistency through
    /// exhaustiveness over BOTH closed enums.
    ///
    /// `pub(crate)` because the byte-discriminator surface is an
    /// implementation detail of the substrate's [`Hash for Atom`]
    /// cache-key contract; exposing it publicly would leak the
    /// cache-key shape through the API without enabling any external
    /// consumer the public projections ([`Atom::kind`], [`Self::label`],
    /// [`Self::sexp_shape`]) don't already serve. Same posture as
    /// [`QuoteForm::hash_discriminator`].
    #[must_use]
    pub(crate) fn hash_discriminator(self) -> u8 {
        match self {
            Self::Symbol => 0,
            Self::Keyword => 1,
            Self::Str => 2,
            Self::Int => 3,
            Self::Float => 4,
            Self::Bool => 5,
        }
    }

    /// Project the typed marker into its matching [`SexpShape`]
    /// variant — `Symbol → SexpShape::Symbol`, `Keyword →
    /// SexpShape::Keyword`, `Str → SexpShape::String`, `Int →
    /// SexpShape::Int`, `Float → SexpShape::Float`, `Bool →
    /// SexpShape::Bool`. ONE projection on the closed-set atomic-
    /// payload algebra that [`crate::domain::sexp_shape`]'s outer-shape
    /// projection routes through for the six atom arms — so the
    /// (Atom variant, SexpShape variant) pairing binds at ONE site on
    /// the typed algebra rather than at six byte-identical inline arms
    /// in [`crate::domain::sexp_shape`]. Direct sibling to
    /// [`QuoteForm::sexp_shape`] — that closed enum carves the
    /// quote-family arms of [`SexpShape`]'s twelve-variant closed set,
    /// while this enum carves the atomic-payload arms.
    ///
    /// Composition law: for every [`Atom`] `a`,
    /// `crate::domain::sexp_shape(&Sexp::Atom(a.clone())) ==
    /// a.kind().sexp_shape()`. Pinned by the cross-projection round-trip
    /// test in this module, so a regression that drifts either side
    /// of the typed algebra (an [`Atom::kind`] arm or this
    /// [`Self::sexp_shape`] arm) surfaces immediately rather than as a
    /// silent operator-facing diagnostic drift at every
    /// `LispError::TypeMismatch.got` slot for an atomic witness.
    ///
    /// Bidirectional dual: the inverse projection
    /// [`crate::error::SexpShape::as_atom_kind`] (12→6, partial)
    /// covers the 6-of-12 carving of [`SexpShape`] this embed
    /// reaches. The pair `(AtomKind::sexp_shape,
    /// SexpShape::as_atom_kind)` forms an `Iso(AtomKind, AtomShape ⊂
    /// SexpShape)`: every typed marker round-trips through the embed
    /// (`AtomKind::sexp_shape(k).as_atom_kind() == Some(k)` for every
    /// `k: AtomKind`), every atom-shape pre-image recovers the typed
    /// marker. The non-atom shapes (`Nil`, `List`, every quote-family
    /// wrapper) form the kernel of the inverse — `as_atom_kind`
    /// returns `None` for them. See [`crate::error::SexpShape::as_atom_kind`]'s
    /// docstring for the composition law's other direction +
    /// disjointness with the quote-family sibling
    /// `SexpShape::as_quote_form`.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the (Atom
    /// variant, SexpShape variant) pairing becomes a TYPE projection
    /// on the substrate algebra rather than six inline arms in
    /// [`crate::domain::sexp_shape`]. A typo or swap at the shape-
    /// projection site is no longer a runtime drift but a compile
    /// error against the typed projection. THEORY.md §II.1 invariant
    /// 2 — free middle; THREE consumers ([`Hash for Atom`] via
    /// [`Self::hash_discriminator`], [`crate::domain::sexp_shape`]
    /// via this method, and the future diagnostic / completion surface
    /// via [`Self::label`]) now route through ONE typed closed-set
    /// match family, so a regression that drifts ONE consumer's
    /// pairing from the others cannot reach the substrate's runtime.
    #[must_use]
    pub fn sexp_shape(self) -> SexpShape {
        match self {
            Self::Symbol => SexpShape::Symbol,
            Self::Keyword => SexpShape::Keyword,
            Self::Str => SexpShape::String,
            Self::Int => SexpShape::Int,
            Self::Float => SexpShape::Float,
            Self::Bool => SexpShape::Bool,
        }
    }
}

// `impl fmt::Display for AtomKind` + `impl std::str::FromStr for AtomKind`
// + `impl crate::ClosedSet for AtomKind` + `pub struct UnknownAtomKind(pub
// String)` are generated by `#[derive(tatara_lisp_derive::ClosedSet)]` on
// the enum declaration above. `label` delegates to the inherent
// `AtomKind::label` via `#[closed_set(via = "label")]` so the
// domain-canonical lowercase-vocabulary projection stays load-bearing (the
// six labels `"symbol" / "keyword" / "string" / "int" / "float" / "bool"`
// match the `SexpShape` atomic-subset labels byte-for-byte AND the
// diagnostic-rendering shape `LispError::TypeMismatch.got` keys on
// verbatim). The `display` flag emits the substrate-wide
// `f.write_str(Self::label(*self))` block. `#[closed_set(generate_unknown =
// "atom kind")]` emits the typed parse-rejection carrier with the
// substrate-wide `Debug + Clone + PartialEq + Eq + thiserror::Error`
// derives and the `#[error("unknown atom kind: {0}")]` annotation
// byte-for-byte; the explicit label pins the pre-lift wording even though
// the auto-derived `pascal_to_spaced_lowercase("AtomKind")` projects to
// the same `"atom kind"` literal.

impl Sexp {
    /// Canonical [`Self::Atom`]-[`Atom::Symbol`] outer constructor —
    /// composes [`Atom::symbol`] (the typed-construct method on the
    /// closed-set [`Atom`] algebra) under the [`Self::Atom`] outer
    /// wrapper. The first of six `Self::Atom(Atom::X(_))` outer
    /// constructors all routing through the typed [`Atom`] construct
    /// family at the inner algebra so the `.into()` coercion + tuple-
    /// variant constructor pair lives at ONE site per kind on the
    /// [`Atom`] algebra rather than at this outer constructor's body.
    /// Sibling-shape lift to the [`Atom::as_X`] /
    /// [`Self::as_X`] composition through [`Self::as_atom`] on the
    /// projection axis: where projections route OUTER `Self::as_X`
    /// through `self.as_atom().and_then(Atom::as_X)`, constructions
    /// route OUTER `Self::X` through `Self::Atom(Atom::X(payload))`.
    ///
    /// Composition law (forward): `Sexp::symbol(s) ==
    /// Sexp::Atom(Atom::symbol(s))` for every `s: impl Into<String>`.
    /// Round-trip law (with the soft-projection sibling): for every
    /// `s: &str`, `Sexp::symbol(s).as_symbol() == Some(s)` — the inner
    /// algebra's section-for-retraction surfaces through the outer
    /// algebra without re-derivation. Same posture across the six
    /// sibling pairs.
    #[must_use]
    pub fn symbol(s: impl Into<String>) -> Self {
        Self::Atom(Atom::symbol(s))
    }
    /// Canonical [`Self::Atom`]-[`Atom::Keyword`] outer constructor —
    /// composes [`Atom::keyword`] under [`Self::Atom`]. See
    /// [`Self::symbol`] for the outer-algebra docstring.
    #[must_use]
    pub fn keyword(s: impl Into<String>) -> Self {
        Self::Atom(Atom::keyword(s))
    }
    /// Canonical [`Self::Atom`]-[`Atom::Str`] outer constructor —
    /// composes [`Atom::string`] under [`Self::Atom`].
    #[must_use]
    pub fn string(s: impl Into<String>) -> Self {
        Self::Atom(Atom::string(s))
    }
    /// Canonical [`Self::Atom`]-[`Atom::Int`] outer constructor —
    /// composes [`Atom::int`] under [`Self::Atom`].
    #[must_use]
    pub fn int(n: i64) -> Self {
        Self::Atom(Atom::int(n))
    }
    /// Canonical [`Self::Atom`]-[`Atom::Float`] outer constructor —
    /// composes [`Atom::float`] under [`Self::Atom`].
    #[must_use]
    pub fn float(n: f64) -> Self {
        Self::Atom(Atom::float(n))
    }
    /// Canonical [`Self::Atom`]-[`Atom::Bool`] outer constructor —
    /// composes [`Atom::boolean`] under [`Self::Atom`].
    #[must_use]
    pub fn boolean(b: bool) -> Self {
        Self::Atom(Atom::boolean(b))
    }

    /// Canonical [`Self::Quote`] outer constructor — composes
    /// [`QuoteForm::wrap`] on the [`QuoteForm::Quote`] marker so the
    /// `Box::new(inner)` allocation + tuple-variant pair lives at ONE
    /// site on the closed-set [`QuoteForm`] algebra rather than at
    /// this outer-constructor body. The first of four `Self::Quote*`
    /// outer constructors all routing through the typed
    /// [`QuoteForm::wrap`] family at the inner algebra — the
    /// quote-family-axis section peer of the six `Self::Atom(Atom::X(_))`
    /// outer constructors ([`Self::symbol`], [`Self::keyword`],
    /// [`Self::string`], [`Self::int`], [`Self::float`],
    /// [`Self::boolean`]) all routing through the typed [`Atom`]
    /// construct family on the atomic-payload axis. Sibling-shape lift
    /// to the [`Self::as_quote_form`] soft-projection sibling on the
    /// projection axis: where the projection soft-decomposes a
    /// quote-family wrapper into `Option<(QuoteForm, &Sexp)>` (surfacing
    /// the typed marker alongside the borrowed inner body), each of
    /// these four typed constructors embeds a fresh inner body under
    /// the typed marker into the matching tuple-variant wrapper.
    ///
    /// Composition law (forward): `Sexp::quote(inner) ==
    /// QuoteForm::Quote.wrap(inner) == Sexp::Quote(Box::new(inner))`
    /// for every `inner: Sexp`. Round-trip law (section-for-retraction
    /// with the soft-projection sibling): `Sexp::quote(inner)
    /// .as_quote_form() == Some((QuoteForm::Quote, &inner))` for every
    /// `inner: Sexp` — the inner algebra's typed constructor pairs
    /// section-for-retraction with the outer algebra's soft
    /// projection, and the marker + inner body cross-projection
    /// preserves identity. Same posture across the four sibling
    /// pairs (`Sexp::quote` / `Sexp::quasiquote` / `Sexp::unquote` /
    /// `Sexp::unquote_splice`).
    ///
    /// Pre-lift the `Self::Quote(Box::new(inner))` welded triple
    /// (`Self::Quote`, `Box::new`, `inner`) appeared inline at every
    /// consumer that builds a quote-family wrapper — well past the ≥2
    /// PRIME-DIRECTIVE trigger once the structural shape is named. The
    /// welded triple already lives at ONE site on the closed-set
    /// [`QuoteForm::wrap`] algebra for the marker-driven consumer path;
    /// this outer constructor binds the per-variant `Sexp::X(Box::new(
    /// inner))` welded triple to ONE typed-algebra method per marker on
    /// the outer [`Sexp`] algebra, so consumers that know the marker at
    /// compile time bind to the typed method directly rather than
    /// re-deriving the `Self::X(Box::new(_))` pair inline. A future
    /// allocation-policy change (e.g. arena-allocated wrappers for
    /// span-aware [`Sexp`]) lands as ONE edit at [`QuoteForm::wrap`]
    /// (the single site the allocation composition lives) and
    /// propagates through these four typed constructors byte-for-byte.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 2 — free middle; the
    /// (QuoteForm variant, [`Sexp`] tuple-variant constructor) pairing
    /// binds at ONE typed-algebra method per marker on the outer
    /// [`Sexp`] algebra regardless of which consumer reaches in.
    /// THEORY.md §VI.1 — generation over composition; the welded
    /// `Self::X(Box::new(_))` triple at every quote-family construct
    /// site regenerates through `QuoteForm::X.wrap(_)` composition over
    /// the typed algebra rather than per-site re-derivation. THEORY.md
    /// §V.1 — knowable platform; the typed-construct family becomes a
    /// TYPE projection on the substrate's outer [`Sexp`] algebra sitting
    /// next to the typed-project family [`Self::as_quote_form`] rather
    /// than as bare tuple-variant constructor + per-site `Box::new`
    /// discipline. A future fifth homoiconic prefix syntax (e.g. syntax
    /// quotation `#'x` for hygienic macros) extends [`QuoteForm::ALL`] +
    /// [`QuoteForm::wrap`]'s arm + this construct family in lockstep,
    /// rustc-enforced through the closed-set exhaustiveness.
    ///
    /// Frontier inspiration: Racket's `(quote x)` /
    /// `(quasiquote x)` / `(unquote x)` / `(unquote-splicing x)` typed
    /// syntactic-form construct face paired one-for-one with the
    /// [`Self::as_quote_form`] closed-set soft-projection sibling on
    /// the outer syntax algebra — the typed-construct + typed-project
    /// algebra dual is closed at one method per direction per marker
    /// on Racket's surface, and the [`Self::quote`] /
    /// [`Self::quasiquote`] / [`Self::unquote`] / [`Self::unquote_splice`]
    /// family is the Rust-typed peer on the closed-set outer [`Sexp`]
    /// algebra with [`QuoteForm::wrap`] standing in for Racket's typed
    /// dispatch face. MLIR's `mlir::OpBuilder::create<QuoteOp>(loc,
    /// inner)` typed-IR wrapper construction paired with
    /// `mlir::dyn_cast<QuoteOp>(op)` on the projection face — the typed
    /// factory + typed downcast pair the IR algebra closes over on
    /// every wrapper op; [`Self::quote`] / [`Self::as_quote_form`] is
    /// the Rust-typed peer on the outer [`Sexp`] algebra with the
    /// closed-set [`QuoteForm`] standing in for MLIR's `OperationName`
    /// taxonomy over the wrapper-op family.
    #[must_use]
    pub fn quote(inner: Sexp) -> Self {
        QuoteForm::Quote.wrap(inner)
    }
    /// Canonical [`Self::Quasiquote`] outer constructor — composes
    /// [`QuoteForm::wrap`] on the [`QuoteForm::Quasiquote`] marker.
    /// See [`Self::quote`] for the outer-algebra docstring.
    #[must_use]
    pub fn quasiquote(inner: Sexp) -> Self {
        QuoteForm::Quasiquote.wrap(inner)
    }
    /// Canonical [`Self::Unquote`] outer constructor — composes
    /// [`QuoteForm::wrap`] on the [`QuoteForm::Unquote`] marker.
    /// See [`Self::quote`] for the outer-algebra docstring.
    #[must_use]
    pub fn unquote(inner: Sexp) -> Self {
        QuoteForm::Unquote.wrap(inner)
    }
    /// Canonical [`Self::UnquoteSplice`] outer constructor — composes
    /// [`QuoteForm::wrap`] on the [`QuoteForm::UnquoteSplice`] marker.
    /// See [`Self::quote`] for the outer-algebra docstring.
    #[must_use]
    pub fn unquote_splice(inner: Sexp) -> Self {
        QuoteForm::UnquoteSplice.wrap(inner)
    }

    pub fn is_list(&self) -> bool {
        matches!(self, Self::List(_))
    }
    pub fn as_list(&self) -> Option<&[Sexp]> {
        match self {
            Self::List(xs) => Some(xs),
            _ => None,
        }
    }

    /// Canonical [`Self::List`] outer constructor — collects an
    /// `impl IntoIterator<Item = Sexp>` into the tuple-variant payload
    /// `Vec<Sexp>` at ONE site on the closed-set [`Sexp`] algebra. The
    /// residual-axis section-for-retraction sibling of the existing
    /// [`Self::as_list`] soft-projection ([`Option<&[Sexp]>`]): where
    /// the projection soft-decomposes a [`Self::List`] arm into its
    /// borrowed inner slice, this constructor embeds a fresh owned
    /// item sequence into the matching tuple-variant wrapper. Sibling
    /// of the atomic-payload construct family ([`Self::symbol`],
    /// [`Self::keyword`], [`Self::string`], [`Self::int`],
    /// [`Self::float`], [`Self::boolean`] — all routing through the
    /// typed [`Atom`] construct family on the 6-of-12 atomic-payload
    /// carving) and the quote-family construct family ([`Self::quote`],
    /// [`Self::quasiquote`], [`Self::unquote`], [`Self::unquote_splice`]
    /// — all routing through the typed [`QuoteForm::wrap`] family on
    /// the 4-of-12 quote-family carving); closes the (construct,
    /// project) algebra dual on the third and final structural carving
    /// of the outer [`Sexp`] closed set — the 2-of-12 residual axis
    /// covering [`Self::Nil`] and [`Self::List`]. [`Self::Nil`] is a
    /// unit variant carrying no payload — the residual-axis
    /// construct family closes at ONE constructor (this method) for
    /// the sole payload-bearing residual arm.
    ///
    /// Composition law (forward): `Sexp::list(items) ==
    /// Sexp::List(items.into_iter().collect::<Vec<Sexp>>())` for every
    /// `items: impl IntoIterator<Item = Sexp>`. Round-trip law
    /// (section-for-retraction with the soft-projection sibling): for
    /// every `items: Vec<Sexp>`, `Sexp::list(items.clone()).as_list()
    /// == Some(items.as_slice())` — the outer algebra's typed
    /// constructor pairs section-for-retraction with the outer
    /// algebra's soft projection, and the borrowed-slice cross-
    /// projection preserves identity. Sibling posture across the
    /// three axis-construct families on the outer [`Sexp`] algebra
    /// (atomic + quote-family + residual).
    ///
    /// Outer-shape composition law: `Sexp::list(items).shape() ==
    /// SexpShape::List` for every `items: impl IntoIterator<Item =
    /// Sexp>` — the residual-arm outer-shape identity binds through
    /// the typed-shape lattice at ONE arm, symmetric with the
    /// quote-family construct family's outer-shape composition
    /// `Sexp::X_variant(inner).shape() == QuoteForm::X.sexp_shape()`
    /// and the atomic construct family's `Sexp::X_atom(payload).shape()
    /// == AtomKind::X.sexp_shape()`. Structural-carving-marker
    /// composition law: `Sexp::list(items).as_structural_kind() ==
    /// Some(StructuralKind::List)` for every `items: impl
    /// IntoIterator<Item = Sexp>` — the residual-axis carving marker
    /// binds through the closed-set [`StructuralKind`] algebra at ONE
    /// arm, symmetric with the atomic-axis's `Sexp::X_atom(payload)
    /// .as_atom_kind() == Some(AtomKind::X)` marker composition.
    ///
    /// Pre-lift the [`Self::List(Vec<Sexp>)`] welded pair
    /// ([`Self::List`] tuple-variant constructor + `Vec<Sexp>`
    /// payload) appeared inline at every consumer that builds a
    /// list-shaped [`Sexp`] value — well past the ≥2 PRIME-DIRECTIVE
    /// trigger once the structural shape is named. Post-lift the
    /// welded pair binds at ONE typed-algebra method on the outer
    /// [`Sexp`] algebra with an `impl IntoIterator<Item = Sexp>`
    /// bound so consumers that have a `Vec<Sexp>`, a `[Sexp; N]`
    /// array, an `iter().cloned()` sequence, a
    /// `.map(...).collect()`-worthy chain, or a
    /// `once(head).chain(tail)` composition can hand the sequence
    /// directly to the algebra without a per-site `.collect::<Vec<
    /// Sexp>>()` coercion. A future allocation-policy change (e.g.
    /// arena-allocated lists for span-aware [`Sexp`]) lands as ONE
    /// edit at this method site and propagates through consumers
    /// byte-for-byte.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 1 — typed entry; the
    /// (list-shaped inner sequence, [`Self::List`] tuple-variant
    /// constructor) pairing binds at ONE typed-algebra method on the
    /// outer [`Sexp`] algebra, closing the outer-algebra construct
    /// family across ALL THREE structural carvings of the [`SexpShape`]
    /// closed set (atomic-payload + quote-family + residual). THEORY.md
    /// §II.1 invariant 2 — free middle; every consumer that has an
    /// owned or iterable sequence of [`Sexp`] and wants to build a
    /// list-shaped wrapper routes through the SAME typed method, so a
    /// regression that drifts one consumer's construction from the
    /// others cannot reach the substrate's runtime. THEORY.md §V.1 —
    /// knowable platform; the typed-construct family becomes a TYPE
    /// projection on the substrate's outer [`Sexp`] algebra sitting
    /// next to the typed-project family [`Self::as_list`] rather than
    /// bare tuple-variant constructor + per-site `Vec<Sexp>` discipline.
    /// THEORY.md §VI.1 — generation over composition; the residual-
    /// arm outer-shape + carving-marker pairings emerge from ONE
    /// typed-algebra composition on the outer [`Sexp`] algebra rather
    /// than from per-consumer per-variant literals.
    ///
    /// Frontier inspiration: Racket's `(list x y z)` typed list-
    /// construct primitive paired one-for-one with `(list? v)` /
    /// `(car v)` / `(cdr v)` predicate/projection siblings on the
    /// same closed-set list shape — the typed-construct + typed-
    /// project algebra dual is closed at one method per direction on
    /// Racket's surface, and [`Self::list`] / [`Self::as_list`] is
    /// the Rust-typed peer on the closed-set outer [`Sexp`] algebra
    /// with `impl IntoIterator<Item = Sexp>` standing in for Racket's
    /// variadic collect face. MLIR's `mlir::OpBuilder::create<
    /// ListOp>(loc, elements)` typed-IR list-op construction paired
    /// with `mlir::dyn_cast<ListOp>(op)` on the projection face —
    /// the typed factory + typed downcast pair the IR algebra closes
    /// over on every list-shaped op; [`Self::list`] / [`Self::as_list`]
    /// is the Rust-typed peer on the outer [`Sexp`] algebra with
    /// [`StructuralKind::List`] standing in for MLIR's `OperationName`
    /// taxonomy over the list-shaped op family.
    #[must_use]
    pub fn list<I: IntoIterator<Item = Sexp>>(items: I) -> Self {
        Self::List(items.into_iter().collect())
    }

    /// Soft projection onto the closed-set [`StructuralKind`] residual
    /// carving marker — the 2-of-12 carving of the [`SexpShape`] algebra
    /// covering [`Self::Nil`] and [`Self::List`] (the outer shapes that
    /// lie OUTSIDE both the atomic-payload carving
    /// [`AtomKind`](crate::error::SexpShape::as_atom_kind) and the
    /// quote-family carving
    /// [`QuoteForm`](crate::error::SexpShape::as_quote_form)). Returns
    /// `Some(StructuralKind::Nil)` for [`Self::Nil`],
    /// `Some(StructuralKind::List)` for [`Self::List`], `None` for
    /// every other outer shape (every [`Self::Atom`] variant, every
    /// quote-family wrapper: [`Self::Quote`], [`Self::Quasiquote`],
    /// [`Self::Unquote`], [`Self::UnquoteSplice`]).
    ///
    /// Sibling soft-projection peer of [`Self::as_quote_form`] (the
    /// soft-decomposition of the four homoiconic prefix wrappers into
    /// `(QuoteForm, &Sexp)`) and [`Self::as_unquote`] (the
    /// soft-decomposition of the two template-substitution wrappers
    /// into `(UnquoteForm, &Sexp)`). Direct value-level peer of the
    /// shape-level projection
    /// [`SexpShape::as_structural_kind`](crate::error::SexpShape::as_structural_kind)
    /// — the pair `(Sexp::as_structural_kind, SexpShape::as_structural_kind)`
    /// binds the (Sexp value, StructuralKind carving marker) pairing at
    /// ONE typed method on each algebra, symmetric with the existing
    /// (Sexp value → AtomKind via
    /// `Sexp::as_atom().map(Atom::kind)`) atomic-axis composition and
    /// the direct (Sexp value → QuoteForm) marker projection
    /// [`Self::as_quote_form`] returns.
    ///
    /// Composition law: `s.as_structural_kind() ==
    /// s.shape().as_structural_kind()` for every `s: &Sexp`. Pre-lift
    /// the residual-carving marker at the value level was reachable
    /// only via the two-step composition
    /// `s.shape().as_structural_kind()` (walking through the full
    /// 12-variant [`SexpShape`] closed set to arrive at the 2-of-12
    /// carving marker); post-lift the composition lands at ONE typed
    /// method on the value algebra — the Nil arm returns `Some(Nil)`
    /// directly and the List arm returns `Some(List)` directly,
    /// matching the residual-carving membership at the value level.
    /// The composition law is pinned by
    /// `sexp_as_structural_kind_agrees_with_shape_as_structural_kind_for_every_variant`
    /// in this module, so a regression that drifts either projection
    /// from the other surfaces immediately.
    ///
    /// Sibling-shape lift to [`Self::is_list`] (the bare List-arm
    /// predicate) and [`Self::is_kwargs_list`] (the narrower
    /// kwargs-shaped List cohort predicate): where `is_list` returns
    /// `true` iff the value inhabits the List arm of the residual
    /// carving, `as_structural_kind` returns the typed carving marker
    /// that binds BOTH residual arms (Nil and List) at ONE typed
    /// projection — the operator answering "which residual arm?"
    /// rather than the bare "is this the List arm?" predicate.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the
    /// (Sexp variant, StructuralKind carving marker) pairing becomes a
    /// TYPE projection on the substrate `Sexp` algebra rather than a
    /// two-step composition through the shape-level projection. A typo
    /// or swap at the value-projection site is no longer a runtime
    /// drift but a compile error against the typed projection.
    /// THEORY.md §VI.1 — generation over composition; the
    /// residual-carving marker projection now lives on the typed
    /// `Sexp` algebra alongside [`Self::as_atom`], [`Self::as_list`],
    /// [`Self::as_quote_form`], [`Self::as_unquote`], completing the
    /// (Sexp value → closed-set carving marker) family at the residual
    /// axis. THEORY.md §II.1 invariant 2 — free middle; every consumer
    /// that needs the residual-carving marker at the value level (a
    /// future `tatara-check` predicate keyed on the Nil/List cohort, a
    /// future LSP structural-navigation filter that keys on the
    /// residual carving, a future typed-rewriter walk over the
    /// residual arm) binds to ONE typed method on the value algebra
    /// rather than a two-step composition through the shape-level
    /// projection.
    ///
    /// Frontier inspiration: MLIR's `mlir::dyn_cast<StructuralOp>(val)`
    /// typed soft-downcast on the residual carving of a closed-set
    /// value algebra — the (value, typed carving marker) pairing lives
    /// at ONE typed projection on the outer value-algebra sibling. The
    /// Rust-typed peer here uses the substrate's outer `Sexp` algebra
    /// with `Sexp::as_structural_kind` closing the residual-carving
    /// cell of the value-level soft-projection surface, symmetric with
    /// the atomic-axis composition through [`Self::as_atom`] and the
    /// quote-family projection [`Self::as_quote_form`].
    #[must_use]
    pub fn as_structural_kind(&self) -> Option<StructuralKind> {
        match self {
            Self::Nil => Some(StructuralKind::Nil),
            Self::List(_) => Some(StructuralKind::List),
            _ => None,
        }
    }

    /// Structural-shape predicate — `true` iff this is a [`Self::List`]
    /// whose items form a non-empty, even-length `(:k v :k v …)` kwargs
    /// sequence with every even-indexed item being an [`Atom::Keyword`].
    /// `false` for every other outer shape ([`Self::Nil`], every
    /// [`Self::Atom`] variant, every quote-family wrapper) and for every
    /// [`Self::List`] that fails the kwargs convention (empty list, odd
    /// length, or any even-indexed non-keyword).
    ///
    /// The structural witness that [`Self::to_json`] will project this
    /// value as [`serde_json::Value::Object`] rather than
    /// [`serde_json::Value::Array`] at the [`Self::List`] arm — the
    /// `(Sexp variant + kwargs shape, JSON canonical-form)` pairing
    /// binds at ONE inherent method on the algebra rather than at a
    /// free function consumers must reach into the `domain` module
    /// path to invoke. Inverse round-trip law: every
    /// [`Self::from_json`] projection of a [`serde_json::Value::Object`]
    /// satisfies this predicate (the [`Self::List`] arm
    /// [`Self::from_json`] builds for an `Object` is non-empty by the
    /// `Object`'s non-empty-keys invariant when present, even-length by
    /// the alternating `:k v` build, and keyword-headed at every even
    /// index by the `Self::keyword(camel_to_kebab(k))` build — except
    /// for the structurally degenerate empty `Object` which projects to
    /// `Sexp::List(vec![])` and returns `false` here, matching
    /// [`Self::to_json`]'s "empty-list ↛ kwargs" gate).
    ///
    /// Composes through [`Self::as_list`] (the structural soft-projection
    /// onto `&[Sexp]`) and [`Atom::as_keyword`] (the typed soft-projection
    /// onto the keyword payload from the [`Atom`] algebra) — the predicate
    /// is rebuilt from already-lifted algebra primitives rather than
    /// inline-matching the [`Self::List`] arm. Sibling-shape predicate
    /// peer of [`Self::is_list`] (the unconditional [`Self::List`]-arm
    /// predicate), with this method narrowing the structural witness to
    /// the kwargs-shaped sub-cohort. The two predicates partition the
    /// list-typed cell of the algebra: every [`Self::List`] either
    /// satisfies `is_kwargs_list` (projects as [`serde_json::Value::Object`]
    /// through [`Self::to_json`]) or does not (projects as
    /// [`serde_json::Value::Array`]).
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition; the
    /// kwargs-shape predicate, previously a `pub(crate)` free function in
    /// `domain.rs` reached across the module boundary by [`Self::to_json`],
    /// is lifted ONE algebra level higher onto the inherent method on
    /// the [`Sexp`] algebra — completing the structural-predicate family
    /// alongside [`Self::is_list`] and the soft-projection family
    /// ([`Self::as_atom`], [`Self::as_list`], [`Self::as_quote_form`]).
    /// THEORY.md §II.1 invariant 2 — free middle; every consumer that
    /// queries "would [`Self::to_json`] project this as `Object`?" (the
    /// `Self::to_json` arm itself, future authoring-tool diagnostics, a
    /// future LSP completion fallback, a future REPL pretty-printer that
    /// chooses between `(…)` and `{…}` rendering, a future `tatara-check`
    /// typed-pattern matcher) routes through ONE inherent algebra method
    /// rather than reaching into the `domain` module path for a free
    /// function. THEORY.md §V.1 — knowable platform; the JSON-format
    /// witness becomes a TYPE projection on the substrate `Sexp` algebra
    /// next to its sibling `Sexp::is_list` / `Sexp::as_list` pair rather
    /// than living in a `domain.rs` `pub(crate)` helper consumers must
    /// import via module path.
    ///
    /// Frontier inspiration: MLIR's `mlir::Operation::hasTrait<T>()` —
    /// typed-IR operations carry their structural traits as inherent
    /// methods on the operation algebra rather than as free functions
    /// in a sibling module; `Sexp::is_kwargs_list` is the
    /// unstructured-Rust peer on the `Sexp` algebra for the
    /// "would-this-project-as-Object" structural trait. Racket's
    /// `(keyword-apply-procedure? stx)` — the syntax-class predicate
    /// that gates a kwargs-style application form's printer / expander
    /// path on the syntax algebra; `Sexp::is_kwargs_list` is the
    /// substrate's peer at the [`Sexp`] layer, with the `as_list().
    /// is_some_and(…)` composition standing in for Racket's
    /// `syntax-parse` pattern matcher.
    #[must_use]
    pub fn is_kwargs_list(&self) -> bool {
        self.as_list().is_some_and(|items| {
            !items.is_empty()
                && items.len().is_multiple_of(2)
                && items.iter().step_by(2).all(|s| s.as_keyword().is_some())
        })
    }

    /// Soft projection onto the inner [`Atom`] payload — `Some(&Atom)`
    /// iff this is a [`Self::Atom`] variant, `None` for every other
    /// outer shape (`Nil`, `List`, `Quote`, `Quasiquote`, `Unquote`,
    /// `UnquoteSplice`). The structural-lift face of the per-atomic-
    /// payload soft-projection family — composes with the typed
    /// [`Atom::as_symbol`] / [`Atom::as_keyword`] / [`Atom::as_string`]
    /// / [`Atom::as_int`] / [`Atom::as_float`] / [`Atom::as_bool`]
    /// projections to give the six `Sexp::as_X` consumers ONE typed
    /// boundary instead of six inline `Self::Atom(Atom::X(s)) => Some(s)`
    /// arms.
    ///
    /// Sibling soft-projection peer of [`Self::as_quote_form`] (the
    /// soft-decomposition of the four homoiconic prefix wrappers into
    /// `(QuoteForm, &Sexp)`) and [`Self::as_list`] (the soft-decomposition
    /// of the structural list constructor into `&[Sexp]`). Together the
    /// three projections (`as_atom`, `as_list`, `as_quote_form`) and
    /// their nullary peer ([`Self::Nil`] via `matches!(self, Self::Nil)`)
    /// cover every outer-shape arm of the `Sexp` algebra: Nil + Atom +
    /// List + 4 quote-family arms = 7 outer shapes, with the typed-
    /// projection set partitioning them by structural axis.
    ///
    /// Composition law binding `Sexp::as_X` to the typed `Atom` algebra:
    /// for every [`Sexp`] `s`,
    /// `s.as_symbol()` (and each `as_keyword` / `as_string` / `as_int` /
    /// `as_bool` sibling) `== s.as_atom().and_then(Atom::as_<variant>)`.
    /// The `Sexp::as_float` consumer specializes through the widening
    /// inline composition `s.as_atom().and_then(|a| a.as_float()
    /// .or_else(|| a.as_int().map(|n| n as f64)))` so the algebra-level
    /// `Atom::as_float` stays strict and the typed-identity
    /// distinction `Int(1)` vs `Float(1.0)` is preserved at the algebra
    /// layer (see [`Atom::as_int`]'s docstring for the discipline).
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition;
    /// the six inline `Self::Atom(Atom::X(s)) => Some(_)` arms across
    /// the `Sexp::as_X` family is past the three-times rule. THEORY.md
    /// §II.1 invariant 2 — free middle; SIX consumers (`as_symbol`,
    /// `as_keyword`, `as_string`, `as_int`, `as_float`, `as_bool`) now
    /// route through ONE typed structural lift (this method) AND ONE
    /// per-variant projection family on the closed-set `Atom` algebra
    /// rather than six byte-identical outer-arm matches each.
    /// THEORY.md §V.1 — knowable platform; the (Sexp variant, inner
    /// payload kind) pairing becomes a TYPE projection on the substrate
    /// algebra rather than six inline arms scattered across the six
    /// `Sexp::as_X` consumers.
    #[must_use]
    pub fn as_atom(&self) -> Option<&Atom> {
        match self {
            Self::Atom(a) => Some(a),
            _ => None,
        }
    }

    /// Soft projection onto the closed-set [`AtomKind`] atomic-payload
    /// carving marker — the 6-of-12 carving of the [`SexpShape`] algebra
    /// covering [`Self::Atom`]'s six per-payload variants ([`Atom::Symbol`],
    /// [`Atom::Keyword`], [`Atom::Str`], [`Atom::Int`], [`Atom::Float`],
    /// [`Atom::Bool`]). Returns `Some(a.kind())` iff this is a
    /// [`Self::Atom`] variant, `None` for every other outer shape
    /// ([`Self::Nil`], [`Self::List`], every quote-family wrapper:
    /// [`Self::Quote`], [`Self::Quasiquote`], [`Self::Unquote`],
    /// [`Self::UnquoteSplice`]).
    ///
    /// Direct value-level peer of the shape-level projection
    /// [`SexpShape::as_atom_kind`](crate::error::SexpShape::as_atom_kind)
    /// — the pair `(Sexp::as_atom_kind, SexpShape::as_atom_kind)` binds
    /// the (Sexp value, AtomKind carving marker) pairing at ONE typed
    /// method on each algebra, closing the atomic-axis cell of the
    /// (Sexp value → carving marker) matrix. Sibling soft-projection
    /// peer of [`Self::as_structural_kind`] (the 2-of-12 residual
    /// carving returning `Option<StructuralKind>`) and
    /// [`Self::as_quote_form`] (the 4-of-12 quote-family carving
    /// returning `Option<(QuoteForm, &Sexp)>`) — post-lift ALL THREE
    /// carvings that partition the twelve outer shapes of the
    /// [`SexpShape`] algebra have a marker-only value-level projection
    /// on `Sexp`: `as_atom_kind` (atomic axis), `as_quote_form`
    /// (quote-family axis, marker + inner), `as_structural_kind`
    /// (residual axis). The `Sexp::as_atom` projection stays available
    /// for consumers that need the inner [`Atom`] payload for further
    /// per-variant typed projection ([`Atom::as_symbol`] et al.); this
    /// projection is the shortcut for consumers that only need the
    /// carving-marker identity.
    ///
    /// Composition laws (dual bindings): `s.as_atom_kind() ==
    /// s.as_atom().map(Atom::kind) == s.shape().as_atom_kind()` for
    /// every `s: &Sexp`. Pre-lift the atomic carving marker at the
    /// value level was reachable only via one of these two-step
    /// compositions — either through the [`Atom`] algebra
    /// (`as_atom().map(Atom::kind)`) or through the shape algebra
    /// (`shape().as_atom_kind()`). Post-lift the projection lands at
    /// ONE typed method on the value algebra, and both compositions
    /// are pinned as agreement laws (see
    /// `sexp_as_atom_kind_agrees_with_as_atom_map_kind_for_every_variant`
    /// and
    /// `sexp_as_atom_kind_agrees_with_shape_as_atom_kind_for_every_variant`
    /// in this module). A regression that drifts any of the three
    /// projections from the others surfaces immediately.
    ///
    /// Symmetric with [`Self::as_structural_kind`]'s shape (returns
    /// just the marker, no inner-payload borrow) — where
    /// [`Self::as_quote_form`] and [`Self::as_unquote`] surface both
    /// the marker AND the wrapped inner `&Sexp` (because the four
    /// quote-family arms and the two substitution arms structurally
    /// carry a boxed inner value), `as_atom_kind` and
    /// `as_structural_kind` return marker-only projections (the atomic
    /// arm's inner payload is heterogeneous across the six variants —
    /// `String` / `i64` / `f64` / `bool` — and the residual arms
    /// carry no or list-heterogeneous payload). Consumers that need
    /// the payload compose through [`Self::as_atom`] +
    /// [`Atom::as_symbol`] et al. (atomic axis) or [`Self::as_list`]
    /// (residual axis); this projection is the payload-agnostic
    /// carving-marker cell.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the (Sexp
    /// variant, AtomKind carving marker) pairing becomes a TYPE
    /// projection on the substrate `Sexp` algebra rather than a
    /// two-step composition through either the [`Atom`] algebra or the
    /// shape algebra. A typo or swap at the value-projection site is
    /// no longer a runtime drift but a compile error against the
    /// typed projection. THEORY.md §VI.1 — generation over composition;
    /// the atomic-carving marker projection now lives on the typed
    /// `Sexp` algebra alongside [`Self::as_atom`], [`Self::as_list`],
    /// [`Self::as_quote_form`], [`Self::as_unquote`],
    /// [`Self::as_structural_kind`], completing the (Sexp value →
    /// closed-set carving marker) family across ALL THREE axes
    /// (atomic + quote-family + structural-residual). THEORY.md §II.1
    /// invariant 2 — free middle; every consumer that needs the
    /// atomic-carving marker at the value level (a future
    /// `tatara-check` predicate keyed on the atomic cohort, a future
    /// LSP structural-navigation filter that keys on the atomic
    /// carving, a future typed-rewriter walk over the atomic arm)
    /// binds to ONE typed method on the value algebra rather than a
    /// two-step composition.
    ///
    /// Sibling posture across the value-level marker family — the
    /// three projections (`as_atom_kind`, `as_quote_form`,
    /// `as_structural_kind`) form a partition of the seven outer-shape
    /// variants of the `Sexp` algebra: for every `s: &Sexp`, EXACTLY
    /// ONE returns `Some(_)` (pinned by the joint sweep
    /// `sexp_as_atom_kind_partitions_outer_shapes_jointly_with_as_quote_form_and_as_structural_kind`
    /// in this module, sibling to the pre-existing partition sweep
    /// keyed on `as_atom` rather than `as_atom_kind`). The value-level
    /// partition-total invariant across the three carvings is the
    /// value-level peer of the shape-level partition-total invariant
    /// (`sexp_shape_partition_is_total_across_atom_quote_structural_carvings`
    /// in error.rs); each axis has BOTH invariants pinned.
    ///
    /// Frontier inspiration: MLIR's `mlir::dyn_cast<AtomOp>(val)` typed
    /// soft-downcast onto the atomic carving of a closed-set value
    /// algebra — the (value, typed carving marker) pairing lives at
    /// ONE typed projection on the outer value-algebra sibling. The
    /// Rust-typed peer here uses the substrate's outer `Sexp` algebra
    /// with `Sexp::as_atom_kind` closing the atomic-carving cell of
    /// the value-level soft-projection surface, symmetric with the
    /// residual-carving projection [`Self::as_structural_kind`] and
    /// the quote-family projection [`Self::as_quote_form`]. Racket's
    /// `(atom? stx)` predicate paired with `(syntax->datum stx)` on
    /// the atomic branch — the substrate's `as_atom_kind` surfaces the
    /// typed witness (`AtomKind`) alongside the predicate verdict in
    /// ONE `Option<AtomKind>` projection.
    #[must_use]
    pub fn as_atom_kind(&self) -> Option<AtomKind> {
        self.as_atom().map(Atom::kind)
    }

    /// Project this [`Sexp`] to its closed-set [`SexpShape`] outer-shape
    /// marker — `Nil → SexpShape::Nil`, `Atom(a) → a.kind().sexp_shape()`,
    /// `List(_) → SexpShape::List`, and each quote-family wrapper routes
    /// through `as_quote_form().map(|(qf, _)| qf.sexp_shape())`. The
    /// outer-shape peer on the [`Sexp`] algebra of [`Atom::kind`] (the
    /// atomic-payload axis) and [`QuoteForm::sexp_shape`] (the
    /// quote-family axis) — completes the substrate's Sexp-shape
    /// projection family by lifting the free-function dispatcher
    /// [`crate::domain::sexp_shape`] onto the typed `Sexp` algebra
    /// alongside its [`Atom`] / [`QuoteForm`] peers.
    ///
    /// Composition law: `s.shape() == crate::domain::sexp_shape(s)` for
    /// every `s: &Sexp`. The free function continues to exist as a thin
    /// delegate (its callers in `domain.rs`'s diagnostic-builder paths,
    /// `compile.rs`'s `TypeMismatch.got` builder, and downstream tests
    /// route through `s.shape()` after this lift), so the (Sexp variant,
    /// SexpShape variant) pairing now binds at ONE inherent method on
    /// the algebra rather than at a free function `domain` consumers
    /// must reach into the module path to invoke.
    ///
    /// Sibling-shape lift to the typed-EXIT projection trio on [`Atom`]
    /// ([`fmt::Display for Atom`], [`Atom::to_json`],
    /// [`Atom::to_iac_forge_sexpr`]) and the typed-ENTRY classifier
    /// ([`Atom::from_lexeme`]): where the atomic-payload algebra carries
    /// its own per-variant projection family at the atomic-payload
    /// level, the `Sexp` algebra carries this single outer-shape
    /// projection that composes through [`Self::as_atom`] +
    /// [`Atom::kind`] (atomic axis) and [`Self::as_quote_form`] (quote-
    /// family axis) — every other arm (`Nil`, `List`) projects to its
    /// own [`SexpShape`] variant directly.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the
    /// (Sexp variant, SexpShape variant) pairing becomes an inherent
    /// algebra projection rather than a free function in `domain.rs`,
    /// so the projection sits next to the rest of the typed `Sexp`
    /// algebra ([`Self::as_atom`], [`Self::as_list`],
    /// [`Self::as_quote_form`], [`Self::head_symbol`],
    /// [`Self::as_call`]) the substrate carries. THEORY.md §II.1
    /// invariant 2 — free middle; every consumer that needs the
    /// outer shape (diagnostic builders at
    /// [`crate::domain::sexp_witness`] / [`crate::domain::missing_head_err`],
    /// [`crate::compile`]'s `TypeMismatch.got` projection, future LSP /
    /// REPL / `tatara-check` typed-pattern matchers) now reaches a
    /// method on the value rather than a free function imported from
    /// `domain`. THEORY.md §VI.1 — generation over composition; the
    /// inline dispatch lifted to [`crate::domain::sexp_shape`] is now
    /// lifted ONE algebra level higher — from the free function to
    /// the inherent method — so a future `Sexp` variant lands at the
    /// algebra's match site without a module-path indirection. A
    /// future extension (e.g. `Sexp::Vector` for `#(...)` reader
    /// syntax, `Sexp::Map` for `{...}`) extends THIS method + the
    /// `SexpShape` algebra + the free function's delegation in
    /// lockstep — exhaustively checked by rustc across the `Sexp`
    /// match.
    ///
    /// Frontier inspiration: MLIR's `mlir::Operation::getName()` —
    /// the typed-IR operation projects through an inherent method
    /// to its closed-set name on the operation algebra; `Sexp::shape`
    /// is the unstructured-Rust peer on the [`Sexp`] algebra for the
    /// outer-shape projection surface, with [`SexpShape`] standing in
    /// for MLIR's `OperationName` taxonomy. Racket's `(syntax-e stx)`
    /// composed with a datum-prim classifier on the closed-set
    /// syntax-taxonomy projects a syntax object to its outer shape via
    /// a single primitive on the syntax algebra; `Sexp::shape` is the
    /// substrate's typed-Rust peer.
    #[must_use]
    pub fn shape(&self) -> SexpShape {
        // Each variant routes through its closed-set carving-marker's
        // `sexp_shape` projection — the atomic-payload carving via
        // `AtomKind::sexp_shape`, the structural-residual carving via
        // `StructuralKind::sexp_shape`, the quote-family carving via
        // `QuoteForm::sexp_shape`. Post-lift the twelve outer-shape
        // arms of the SexpShape closed set are reached through THREE
        // carving-marker `sexp_shape` projections (6 + 2 + 4 = 12),
        // symmetric across the partition — no arm hits a raw
        // `SexpShape::*` literal here. A future thirteenth variant
        // (e.g. `Sexp::Vector` for `#(...)` reader syntax) extends the
        // carving-marker family the same way and lands at one arm
        // here + one carving-marker `sexp_shape` arm in lockstep.
        match self {
            Self::Nil => StructuralKind::Nil.sexp_shape(),
            Self::Atom(a) => a.kind().sexp_shape(),
            Self::List(_) => StructuralKind::List.sexp_shape(),
            Self::Quote(_) | Self::Quasiquote(_) | Self::Unquote(_) | Self::UnquoteSplice(_) => {
                let (qf, _) = self.expect_quote_form();
                qf.sexp_shape()
            }
        }
    }

    /// Project this `Sexp` to its [`SexpWitness`] — the typed joint
    /// identity pairing the structural [`SexpShape`] with the
    /// renderable [`Sexp::Display`] projection in ONE owned value.
    /// The joint-identity peer on the [`Sexp`] algebra of
    /// [`Self::shape`] (the structural-shape-only projection) and
    /// [`fmt::Display for Sexp`] (the rendered-literal-only
    /// projection) — completes the substrate's Sexp-projection
    /// family by lifting the free-function dispatcher
    /// [`crate::domain::sexp_witness`] onto the typed `Sexp` algebra
    /// alongside its [`Self::shape`] peer.
    ///
    /// Composition law: `s.witness() ==
    /// crate::domain::sexp_witness(s)` for every `s: &Sexp`. The
    /// free function continues to exist as a thin delegate (its
    /// callers in `macro_expand.rs`'s 8 typed-entry rejection
    /// builders, `domain.rs`'s `missing_head_err` caller +
    /// `rewriter_non_list_err` typed-exit builder, and downstream
    /// tests route through `s.witness()` after this lift), so the
    /// (Sexp variant, SexpWitness identity) pairing now binds at
    /// ONE inherent method on the algebra rather than at a free
    /// function `domain` consumers must reach into the module path
    /// to invoke. Body composes the two algebra-level projections
    /// — `self.shape()` for the structural identity, `self.to_string()`
    /// for the renderable identity — into ONE
    /// [`SexpWitness::new`] call. Pre-lift the dispatcher lived as
    /// a free function in `domain.rs`; post-lift the canonical site
    /// is the inherent method and the free function delegates
    /// (mirrors the [`Self::shape`] lift in 121bb60 exactly).
    ///
    /// Sibling-shape lift to [`Self::shape`] (the structural-shape
    /// projection): where `shape()` carries the typed-shape axis on
    /// the `Sexp` algebra, `witness()` carries the JOINT typed-shape
    /// and renderable-literal axis — the typed identity an authoring
    /// tool diagnostic owes the operator AT the typed-entry or
    /// typed-exit rejection boundary. Every rejection-builder
    /// helper in `macro_expand.rs` that previously projected `&Sexp`
    /// through `crate::domain::sexp_witness(_)` at the variant
    /// boundary now reaches a method on the value rather than a
    /// free function imported from `domain`.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the
    /// (Sexp variant, SexpWitness identity) pairing becomes an
    /// inherent algebra projection rather than a free function in
    /// `domain.rs`, so the projection sits next to the rest of the
    /// typed `Sexp` algebra ([`Self::shape`], [`Self::as_atom`],
    /// [`Self::as_list`], [`Self::as_quote_form`],
    /// [`Self::head_symbol`], [`Self::as_call`]) the substrate
    /// carries. THEORY.md §II.1 invariant 2 — free middle; every
    /// consumer that needs the typed joint identity at a
    /// rejection-boundary slot (`NonSymbolUnquoteTarget.got`,
    /// `SpliceOutsideList.got`, `NonSymbolParam.got`,
    /// `RestParamMissingName.got`, `RestParamTrailingTokens.first`,
    /// `OptionalParamMalformed.got`, `DefmacroNonSymbolName.got`,
    /// `DefmacroNonListParams.got`, `MissingHeadSymbol.got`,
    /// `RewriterNonList.got`, future LSP / REPL / `tatara-check`
    /// typed-pattern matchers) now reaches a method on the value
    /// rather than a free function imported from `domain`.
    /// THEORY.md §VI.1 — generation over composition; the inline
    /// dispatch lifted to [`crate::domain::sexp_witness`] is now
    /// lifted ONE algebra level higher — from the free function
    /// to the inherent method — completing the Sexp-projection
    /// family alongside [`Self::shape`]. A future `Sexp` variant
    /// extension (e.g. `Sexp::Vector` for `#(...)` reader syntax,
    /// `Sexp::Map` for `{...}`) reaches this method through the
    /// already-lifted [`Self::shape`] + [`fmt::Display for Sexp`]
    /// pair — no new arm needed here.
    ///
    /// Frontier inspiration: MLIR's diagnostic builder pattern —
    /// `op.emitOpError() << op` projects the offending operation
    /// through inherent methods (`getName()`, `print()`) into ONE
    /// diagnostic value; `Sexp::witness` is the unstructured-Rust
    /// peer on the [`Sexp`] algebra for the joint typed-shape +
    /// renderable-literal projection surface, with [`SexpWitness`]
    /// standing in for MLIR's `InFlightDiagnostic` typed payload.
    #[must_use]
    pub fn witness(&self) -> SexpWitness {
        SexpWitness::new(self.shape(), self.to_string())
    }

    /// Project this `Sexp` to its stable, human-readable outer-shape
    /// label — the `&'static str` axis on the [`Sexp`] algebra. Lifts
    /// the free-function dispatcher [`crate::domain::sexp_type_name`]
    /// onto the typed `Sexp` algebra alongside its [`Self::shape`] /
    /// [`Self::witness`] / [`Self::to_json`] / [`Self::from_json`]
    /// sibling projections, completing the substrate's
    /// Sexp-projection family at the canonical-label axis the way
    /// [`Self::shape`] completes the typed-shape axis and
    /// [`fmt::Display for Sexp`] completes the canonical-string axis.
    ///
    /// Composition law: `s.type_name() == s.shape().label() ==
    /// crate::domain::sexp_type_name(s)` for every `s: &Sexp`.
    /// Pre-lift the projection lived as a free function in
    /// `domain.rs` consumers (in particular the `LispError::TypeMismatch`
    /// `got` slot in `compile.rs` and the legacy substring-grep
    /// rejection-message tests) reached across module boundaries to
    /// invoke; post-lift the canonical site is the inherent method on
    /// the [`Sexp`] algebra and the free function delegates so existing
    /// callers continue to compile. Body composes through
    /// [`Self::shape`] + [`SexpShape::label`] so a future `Sexp`
    /// variant (e.g. `Sexp::Vector` for `#(...)` reader syntax,
    /// `Sexp::Map` for `{...}`) lands at one extension site
    /// ([`Self::shape`]'s exhaustive arm) rather than a parallel
    /// `&'static str` match — the projection is structurally derived,
    /// not duplicated.
    ///
    /// Sibling-shape lift to [`Self::shape`] (the typed-shape
    /// projection): where `shape()` carries the typed
    /// [`SexpShape`] identity (matchable, exhaustive across `Sexp`
    /// variants), `type_name()` carries the `&'static str` literal
    /// the rendered diagnostic surface wants (still derived from
    /// the typed identity, but flattened through
    /// [`SexpShape::label`] for substring-grep callers and the
    /// `TypeMismatch.got` slot). The `&'static str` lifetime makes
    /// the projection cheap to embed in any error variant without
    /// allocation.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the
    /// (Sexp variant, `&'static str` label) pairing becomes an
    /// inherent algebra projection rather than a free function in
    /// `domain.rs`, so the projection sits next to the rest of the
    /// typed `Sexp` algebra ([`Self::shape`], [`Self::witness`],
    /// [`Self::to_json`], [`Self::from_json`], [`Self::as_atom`],
    /// [`Self::as_list`], [`Self::as_quote_form`],
    /// [`Self::head_symbol`], [`Self::as_call`]) the substrate
    /// carries. THEORY.md §II.1 invariant 2 — free middle; every
    /// consumer that needs the outer-shape label
    /// (`LispError::TypeMismatch.got` projection in `compile.rs`,
    /// legacy substring-grep rejection-message tests, future LSP /
    /// REPL diagnostic surfaces) now reaches a method on the value
    /// rather than a free function imported from `domain`.
    /// THEORY.md §VI.1 — generation over composition; the inline
    /// `s.shape().label()` recipe lifted to
    /// [`crate::domain::sexp_type_name`] is now lifted ONE algebra
    /// level higher — from the free function to the inherent
    /// method — completing the Sexp-projection family alongside
    /// [`Self::shape`] / [`Self::witness`] / [`Self::to_json`] /
    /// [`Self::from_json`]. The `domain.rs` `sexp_*` free-function
    /// namespace is now structurally reserved for free functions
    /// that genuinely need a `domain`-module reach (registry
    /// dispatch, kwargs gates, registry suggestions), not
    /// algebra-layer projections.
    ///
    /// Frontier inspiration: MLIR's `mlir::Operation::getName()`
    /// composed with `OperationName::getStringRef()` — the typed-IR
    /// operation projects through inherent methods to its closed-set
    /// label on the operation algebra; `Sexp::type_name` is the
    /// unstructured-Rust peer on the [`Sexp`] algebra for the
    /// canonical-label projection surface, with [`SexpShape::label`]
    /// standing in for MLIR's `OperationName::getStringRef` second
    /// hop. Racket's `(syntax-name stx)` — the typed inverse of
    /// `(syntax-e stx)` on the syntax algebra; `Sexp::type_name`
    /// composes the typed-shape projection with its closed-set
    /// label projection at the inherent-method site rather than
    /// the typeclass-method site, matching pleme-io's
    /// "rust-typed, not trait-typed" idiom for closed-set algebras.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        self.shape().label()
    }

    /// Project this `Sexp` to its canonical [`serde_json::Value`]
    /// rendering — the typed-algebra peer of [`Atom::to_json`] at the
    /// `Sexp` layer. Lifts the free-function dispatcher
    /// [`crate::domain::sexp_to_json`] onto the typed `Sexp` algebra
    /// alongside its [`Self::shape`] / [`Self::witness`] sibling
    /// projections, completing the JSON-projection axis at the
    /// algebra layer the way [`fmt::Display for Sexp`] completes the
    /// canonical-string axis. The free function continues to exist
    /// as a thin delegate (its callers in `tatara-lisp-derive`'s
    /// derive output route through it via the
    /// `crate::domain::sexp_to_json` import); the
    /// `from_value_with_path` private helper in `domain.rs` and the
    /// recursive sub-calls inside this method route through the
    /// inherent method directly so the canonical-site indirection
    /// disappears at every internal callsite.
    ///
    /// Rules (preserve byte-identical pre-lift behavior at the
    /// `sexp_to_json` callsite):
    ///   - [`Self::Nil`] → [`serde_json::Value::Null`].
    ///   - [`Self::Atom`] → [`Atom::to_json`] (the typed-algebra
    ///     peer at the atomic-payload layer; pinned by
    ///     `sexp_to_json_atom_arms_route_through_atom_to_json` in
    ///     `domain.rs`).
    ///   - [`Self::List`] with kwargs shape `(:k v :k v …)` →
    ///     [`serde_json::Value::Object`] keyed by
    ///     [`crate::domain::kebab_to_camel`] of each `:k`'s name.
    ///     A duplicate kebab→camel key inside any nested kwargs-list
    ///     fails with [`crate::domain::duplicate_kwarg`] — same
    ///     typed-entry posture
    ///     [`crate::domain::parse_kwargs`] takes at the top level.
    ///   - [`Self::List`] otherwise → [`serde_json::Value::Array`]
    ///     mapping each element through this method recursively.
    ///   - [`Self::Quote`] / [`Self::Quasiquote`] / [`Self::Unquote`]
    ///     / [`Self::UnquoteSplice`] → recurse on the inner via
    ///     [`Self::expect_quote_form`] (strips the wrapper; the
    ///     round-trip via [`crate::domain::json_to_sexp`] re-emits
    ///     the inner without an enclosing wrapper). All four arms
    ///     route through ONE [`Self::as_quote_form`]-derived
    ///     projection so the per-variant pairing binds at ONE site
    ///     on the [`QuoteForm`] algebra rather than four
    ///     byte-identical inline arms.
    ///
    /// Composition law: `s.to_json() == crate::domain::sexp_to_json(s)`
    /// for every `s: &Sexp`. Pre-lift the dispatcher lived as a free
    /// function in `domain.rs`; post-lift the canonical site is the
    /// inherent method and the free function delegates (same lift
    /// posture as [`Self::shape`] in 121bb60 and [`Self::witness`]
    /// in a427e3b).
    ///
    /// Sibling-shape lift to [`Self::shape`] (the structural-shape
    /// projection), [`Self::witness`] (the joint structural-shape +
    /// renderable-literal projection), and [`fmt::Display for Sexp`]
    /// (the renderable-literal projection): where those three carry
    /// the Lisp-canonical-form / structural-identity axes on the
    /// algebra, `to_json` carries the JSON canonical-form axis. The
    /// substrate's `Sexp` algebra now binds ALL THREE canonical-form
    /// projection surfaces (Lisp Display, JSON, and the feature-gated
    /// iac-forge `From<&Sexp> for SExpr`) at the algebra layer, with
    /// per-variant atomic rendering composed through the corresponding
    /// [`Atom`] projection family (`Atom::Display`, [`Atom::to_json`],
    /// `Atom::to_iac_forge_sexpr`).
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition;
    /// the inline dispatch the prior runs lifted onto
    /// [`crate::domain::sexp_to_json`] (the free function) is now
    /// lifted ONE algebra level higher — from the free function to
    /// the inherent method — completing the Sexp-projection family
    /// alongside [`Self::shape`] and [`Self::witness`]. THEORY.md
    /// §II.1 invariant 2 — free middle; the typed-exit JSON
    /// projection (every consumer that round-trips a Sexp through
    /// `serde_json::from_value::<T>` for typed-domain
    /// deserialization, the typed-rewriter at
    /// [`crate::domain::TypedRewriter`], the derive macro's
    /// `compile_from_args` fallthrough, and any future canonical-
    /// form surface) all route through ONE inherent algebra method
    /// rather than reach into the `domain` module path for a free
    /// function. THEORY.md §V.1 — knowable platform; a future
    /// `Sexp` variant extension (e.g. `Sexp::Vector` for `#(...)`
    /// reader syntax, `Sexp::Map` for `{...}`) reaches this method
    /// through the already-lifted [`Self::as_quote_form`] +
    /// [`Atom::to_json`] pair — one arm added here for the new
    /// outer-shape variant; rustc enforces the per-variant body is
    /// named.
    ///
    /// Frontier inspiration: MLIR's `mlir::AsmPrinter::printOp` —
    /// the typed-IR printer dispatches on the closed-set `Op` so
    /// every printer body for an op lives at ONE implementation site;
    /// `Sexp::to_json` is the unstructured-Rust peer on the `Sexp`
    /// algebra for the JSON canonical-form surface (where
    /// [`fmt::Display for Sexp`] is the Lisp-canonical-form peer
    /// and `From<&Sexp> for iac_forge::SExpr` is the
    /// canonical-attestation-form peer). Racket's `(syntax->datum
    /// stx)` then a serializer over the datum prim — `to_json` is
    /// the substrate's serializer at the Sexp layer composed
    /// through [`Atom::to_json`] at the atomic-payload layer, with
    /// the closed-set [`AtomKind`] standing in for Racket's
    /// datum-prim taxonomy.
    pub fn to_json(&self) -> crate::error::Result<serde_json::Value> {
        Ok(match self {
            Self::Nil => serde_json::Value::Null,
            Self::Atom(a) => a.to_json(),
            Self::List(items) => {
                if self.is_kwargs_list() {
                    let mut map = serde_json::Map::with_capacity(items.len() / 2);
                    let mut i = 0;
                    while i + 1 < items.len() {
                        if let Some(k) = items[i].as_keyword() {
                            let value = items[i + 1].to_json()?;
                            if map
                                .insert(crate::domain::kebab_to_camel(k), value)
                                .is_some()
                            {
                                return Err(crate::domain::duplicate_kwarg(k));
                            }
                            i += 2;
                        } else {
                            break;
                        }
                    }
                    serde_json::Value::Object(map)
                } else {
                    serde_json::Value::Array(
                        items
                            .iter()
                            .map(Self::to_json)
                            .collect::<crate::error::Result<Vec<_>>>()?,
                    )
                }
            }
            Self::Quote(_) | Self::Quasiquote(_) | Self::Unquote(_) | Self::UnquoteSplice(_) => {
                let (_, inner) = self.expect_quote_form();
                inner.to_json()?
            }
        })
    }

    /// Inverse of [`Self::to_json`] — project a [`serde_json::Value`] back
    /// onto a [`Sexp`]. The closed-set [`serde_json::Value`] discriminator
    /// maps directly onto the corresponding [`Sexp`] constructor:
    ///
    ///   - [`serde_json::Value::Null`] → [`Self::Nil`].
    ///   - [`serde_json::Value::Bool`] → [`Self::boolean`].
    ///   - [`serde_json::Value::Number`] → [`Self::int`] when the value
    ///     fits an [`i64`], otherwise [`Self::float`] when it fits an
    ///     [`f64`]; the structural impossibility "neither i64 nor f64"
    ///     collapses to [`Self::int(0)`](Self::int) as a typed floor —
    ///     [`serde_json::Number`]'s closed-set discriminator excludes
    ///     this case in practice (every [`serde_json::Number`] is either
    ///     i64-fitting, u64-fitting projected through f64, or f64-fitting
    ///     directly), but the typed floor stays explicit so a future
    ///     `serde_json` extension does not silently misroute. Mirror of
    ///     [`Atom::to_json`]'s [`Self::int`] / [`Self::float`] bifurcation.
    ///   - [`serde_json::Value::String`] → [`Self::string`]. The
    ///     `serde_json::Value::String` discriminator is type-erased — a
    ///     serde-projected symbol AND a serde-projected keyword AND a
    ///     genuine string literal ALL inhabit it on the JSON side — so
    ///     the back-projection chooses [`Self::string`] as the lossless
    ///     floor for the `Atom::Symbol` / `Atom::Keyword` / `Atom::Str`
    ///     three-way collapse. Consumers that need the symbol-vs-string
    ///     distinction must preserve it BEFORE the JSON round-trip
    ///     (e.g. through a typed enum's serde projection rather than a
    ///     raw `Sexp`-to-`JValue` round-trip).
    ///   - [`serde_json::Value::Array`] → [`Self::List`] mapping each
    ///     element through this method recursively.
    ///   - [`serde_json::Value::Object`] → [`Self::List`] of alternating
    ///     `:key value` pairs in [`serde_json::Map`]'s iteration order
    ///     (sorted by key under `serde_json`'s default `BTreeMap`
    ///     backing; insertion order under the optional `preserve_order`
    ///     feature, which the substrate does NOT enable today), with
    ///     each JSON key projected through
    ///     [`crate::domain::camel_to_kebab`] to recover the `:k`'s
    ///     kebab-case authoring shape and each JSON value recursed
    ///     through this method. Inverse of [`Self::to_json`]'s
    ///     [`Self::List`] kwargs-shape arm: that arm projects
    ///     `:k v :k v …` into a JSON object via
    ///     [`crate::domain::kebab_to_camel`]; this arm projects the
    ///     object back into a `Self::List` of alternating keyword /
    ///     value via the inverse [`crate::domain::camel_to_kebab`].
    ///
    /// Composition law: `Self::from_json(&s.to_json()?)` projects back
    /// to a `Sexp` whose [`Self::to_json`] re-projection produces the
    /// SAME `JValue` (modulo the lossy `Symbol` / `Keyword` / `Str`
    /// three-way collapse documented above; for the round-trippable
    /// subset, `Sexp::Nil`, the six [`Atom`] kinds within their
    /// discriminator class, and recursively `Sexp::List` of round-
    /// trippable elements, the law holds byte-for-byte).
    ///
    /// Sibling-lift posture: this method mirrors the prior
    /// [`crate::domain::sexp_to_json`] → [`Self::to_json`] (commit
    /// 875ee3b) / [`crate::domain::sexp_shape`] → [`Self::shape`]
    /// (commit 121bb60) / [`crate::domain::sexp_witness`] →
    /// [`Self::witness`] (commit a427e3b) family of lifts, all of which
    /// promoted a free function in `domain.rs` to the inherent-method
    /// canonical site on the [`Sexp`] algebra. Pre-lift the
    /// `json_to_sexp` dispatcher lived in `domain.rs` as the canonical
    /// site; post-lift this inherent method is the canonical site and
    /// the free function delegates so every existing caller continues
    /// to compile.
    ///
    /// Sibling-shape lift on the round-trip closure: the substrate's
    /// `Sexp` ↔ `serde_json::Value` round-trip now lives entirely as
    /// two inherent methods on the [`Sexp`] algebra — [`Self::to_json`]
    /// (forward) and [`Self::from_json`] (inverse). Consumers that
    /// previously round-tripped a typed value through Lisp forms via
    /// `domain::sexp_to_json` + `domain::json_to_sexp` now bind to ONE
    /// algebra (the inherent-method family) rather than reaching across
    /// the `domain` module path for two free functions. A future
    /// canonical-form surface (e.g., a YAML round-trip via
    /// [`serde_yaml`], a Nix-expression round-trip via the typed Nix
    /// surface in `tatara-nix`) hangs off the SAME `Sexp` algebra at
    /// `Self::to_yaml` / `Self::from_yaml` / `Self::to_nix` /
    /// `Self::from_nix` — the naming pattern is now structurally
    /// established by this pair.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition;
    /// the inline `json_to_sexp` dispatcher in `domain.rs` is lifted
    /// ONE algebra level higher (from free function to inherent
    /// method), completing the Sexp ↔ JValue round-trip closure
    /// alongside [`Self::to_json`]. THEORY.md §V.1 — knowable
    /// platform; the inverse projection becomes a NAMED primitive on
    /// the substrate's `Sexp` algebra rather than a `domain`-module
    /// free function consumers reach across module boundaries to call.
    /// THEORY.md §II.1 invariant 2 — free middle; every consumer that
    /// round-trips through JSON (the typed-rewriter at
    /// [`crate::domain::TypedRewriter`], the derive macro's
    /// `compile_from_args` JSON fallthrough, the test round-trip
    /// fixtures) routes through ONE inherent algebra method — the
    /// typed round-trip closure is structurally complete on the
    /// `Sexp` algebra.
    ///
    /// Frontier inspiration: MLIR's `mlir::parseAttribute(str, ctx)` —
    /// the typed-IR parser inverse of `printAttribute` lives on the
    /// same `Attribute` algebra as its printer dual; the substrate's
    /// [`Self::from_json`] is the unstructured-Rust peer on the
    /// `Sexp` algebra for the JSON canonical-form inverse, paired
    /// with [`Self::to_json`] as the closed round-trip. Racket's
    /// `(datum->syntax stx datum)` — the round-trip inverse of
    /// `(syntax->datum stx)`, projected at the `datum` algebra layer;
    /// `Self::from_json` is the substrate's peer at the `Sexp` layer
    /// (one algebra level lower than Racket's `syntax` wrapper).
    #[must_use]
    pub fn from_json(v: &serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => Self::Nil,
            serde_json::Value::Bool(b) => Self::boolean(*b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Self::int(i)
                } else if let Some(f) = n.as_f64() {
                    Self::float(f)
                } else {
                    Self::int(0)
                }
            }
            serde_json::Value::String(s) => Self::string(s.clone()),
            serde_json::Value::Array(items) => {
                Self::List(items.iter().map(Self::from_json).collect())
            }
            serde_json::Value::Object(map) => {
                let mut out = Vec::with_capacity(map.len() * 2);
                for (k, v) in map {
                    out.push(Self::keyword(crate::domain::camel_to_kebab(k)));
                    out.push(Self::from_json(v));
                }
                Self::List(out)
            }
        }
    }

    pub fn as_symbol(&self) -> Option<&str> {
        self.as_atom().and_then(Atom::as_symbol)
    }
    pub fn as_keyword(&self) -> Option<&str> {
        self.as_atom().and_then(Atom::as_keyword)
    }
    pub fn as_string(&self) -> Option<&str> {
        self.as_atom().and_then(Atom::as_string)
    }
    pub fn as_int(&self) -> Option<i64> {
        self.as_atom().and_then(Atom::as_int)
    }
    /// `Some(f)` for `Atom::Float(f)`, AND `Some(n as f64)` for
    /// `Atom::Int(n)` — caller convenience at the numeric-kwarg
    /// boundary. The Int-widening face lives at this consumer layer
    /// rather than at [`Atom::as_float`] (strict per the typed-identity
    /// discipline pinned at [`Atom::as_int`]'s docstring); the typed
    /// soft-projection algebra on `Atom` stays strict, and the
    /// `Sexp::as_float` consumer composes the strict typed projection
    /// with a fallback widening branch on `Atom::as_int`.
    pub fn as_float(&self) -> Option<f64> {
        let a = self.as_atom()?;
        a.as_float().or_else(|| a.as_int().map(|n| n as f64))
    }
    pub fn as_bool(&self) -> Option<bool> {
        self.as_atom().and_then(Atom::as_bool)
    }
    /// `foo` or `"foo"` — useful for names that may be authored either way.
    ///
    /// Structural-lift composition: routes through [`Sexp::as_atom`] + the
    /// algebra-level [`Atom::as_symbol_or_string`] union projection — the
    /// same `as_atom().and_then(Atom::as_X)` composition pattern
    /// [`Sexp::as_symbol`] / [`Sexp::as_keyword`] / [`Sexp::as_string`] /
    /// [`Sexp::as_int`] / [`Sexp::as_bool`] route through on the
    /// per-variant axis. Lifts the disjunctive
    /// `self.as_symbol().or_else(|| self.as_string())` composition at this
    /// site's pre-lift body (TWO `Sexp::as_atom` traversals — one per
    /// per-variant projection) onto ONE typed-algebra union projection
    /// reached via ONE `Sexp::as_atom` traversal.
    ///
    /// Composition law: `s.as_symbol_or_string() == s.as_atom().and_then(Atom::as_symbol_or_string)`
    /// for every [`Sexp`] `s`. See [`Atom::as_symbol_or_string`] for the
    /// algebra-level peer's docstring (per-variant family completion +
    /// theory grounding).
    pub fn as_symbol_or_string(&self) -> Option<&str> {
        self.as_atom().and_then(Atom::as_symbol_or_string)
    }

    /// The symbol in operator position — `Some(s)` iff this is a non-empty
    /// list whose first element is a symbol (`(defpoint …)` → `Some("defpoint")`).
    /// `None` for every other shape: a non-list (`foo`, `5`, `:kw`), the
    /// empty list `()`, and a list whose head is not a symbol (`(5 …)`,
    /// `(:kw …)`, `((nested) …)`).
    ///
    /// This is the *operator-position projection* — the structural query
    /// every form-dispatch site in the substrate keys on: "what operator
    /// does this form invoke?" Macroexpansion (`Expander::expand` looks up
    /// the head against the macro table; `macro_def_from` reads it to
    /// recognize a `defmacro` head) and the typed compilers
    /// (`compile_typed` / `compile_named_from_forms` match it against
    /// `T::KEYWORD`) all asked the same `self.as_list()?.first()?.as_symbol()`
    /// question inline. Naming it once makes "operator position" a primitive
    /// of the `Sexp` algebra rather than four byte-identical inline chains.
    ///
    /// This is the SOFT face of operator-position dispatch — it answers
    /// "is this form an invocation of some operator?" and yields `None`
    /// (skip / fall through) for everything that isn't, with no diagnostic.
    /// Its STRICT sibling is `TataraDomain::compile_from_sexp`, which on a
    /// matched-arity form distinguishes the empty-list and
    /// present-but-not-a-symbol head sub-modes to emit a rich
    /// `MissingHeadSymbol` rejection. The two are the dispatch (`head_symbol`)
    /// and the gate (`compile_from_sexp`) faces of the same projection;
    /// keeping both lets a site choose "skip silently" or "reject loudly"
    /// without re-deriving the head.
    ///
    /// `head_symbol` is the operator projection of [`Sexp::as_call`]: it
    /// keeps the head and discards the argument tail. The
    /// `as_list()?.first()?.as_symbol()` chain lives in ONE place
    /// (`as_call`); this is its first component.
    pub fn head_symbol(&self) -> Option<&str> {
        self.as_call().map(|(head, _)| head)
    }

    /// Decompose a call form into its operator and argument tail —
    /// `Some((op, args))` iff this is a non-empty list whose first element
    /// is a symbol, where `op` is that head symbol and `args` is the
    /// remaining elements (`&self[1..]`, possibly empty). `None` for every
    /// shape `head_symbol` rejects: a non-list, the empty list, and a list
    /// whose head is present but not a symbol.
    ///
    /// This is the *call-form decomposition* — the structural shape of a
    /// Lisp invocation: an operator applied to an argument tail. It pairs
    /// the operator-position projection (`head_symbol`) with the argument
    /// tail every dispatch site reads immediately after matching the
    /// operator. Macroexpansion (`Expander::expand`) applies the matched
    /// macro to `&list[1..]`; the typed compilers (`compile_typed`,
    /// `compile_named_from_forms`) feed `&list[1..]` into
    /// `T::compile_from_args`. Before this query each site bound
    /// `as_list()` for the tail AND independently called `head_symbol()`
    /// (which itself re-derives `as_list().first()`) for the operator —
    /// two traversals of the same list, two projections. `as_call` yields
    /// both from one match, so the operator and its arguments can never
    /// drift out of agreement at a dispatch site.
    ///
    /// Soft face, like `head_symbol`: it answers "is this an invocation of
    /// some operator, and what are its arguments?" and yields `None` (skip
    /// / fall through) for everything that isn't, with no diagnostic. The
    /// strict gate sibling is `TataraDomain::compile_from_sexp`, which
    /// distinguishes the empty-list and non-symbol-head sub-modes to reject
    /// loudly.
    pub fn as_call(&self) -> Option<(&str, &[Sexp])> {
        let list = self.as_list()?;
        let head = list.first()?.as_symbol()?;
        Some((head, &list[1..]))
    }

    /// Canonical call-form outer constructor — composes the atomic-
    /// payload construct family's [`Self::symbol`] (the head-position
    /// construct on the 6-of-12 atomic carving of [`SexpShape`]) with
    /// the residual-axis construct family's [`Self::list`] (via
    /// `std::iter::once(head_sexp).chain(args)`) to build a symbol-
    /// headed list-shaped [`Sexp`] value at ONE site on the closed-set
    /// [`Sexp`] algebra. The call-form section-for-retraction sibling
    /// of the existing [`Self::as_call`] soft-projection ([`Option<(&
    /// str, &[Sexp])>`]): where the projection soft-decomposes a
    /// symbol-headed list into its head symbol and argument tail, this
    /// constructor embeds a fresh (head string, item sequence) pair
    /// into the matching call-shaped wrapper.
    ///
    /// Composition sibling of the atomic-payload construct family
    /// ([`Self::symbol`], [`Self::keyword`], [`Self::string`],
    /// [`Self::int`], [`Self::float`], [`Self::boolean`] — routing
    /// through the typed [`Atom`] family on the 6-of-12 atomic carving),
    /// the quote-family construct family ([`Self::quote`],
    /// [`Self::quasiquote`], [`Self::unquote`], [`Self::unquote_splice`]
    /// — routing through the typed [`QuoteForm::wrap`] family on the
    /// 4-of-12 quote-family carving), and the residual-axis construct
    /// [`Self::list`] (routing owned or iterable item sequences into
    /// the tuple-variant on the 2-of-12 residual carving): those close
    /// the (construct, project) algebra dual on their respective
    /// STRUCTURAL carvings; this closes the (construct, project)
    /// algebra dual on the SYMBOL-HEADED-LIST TYPED DECOMPOSITION — the
    /// load-bearing shape every Lisp invocation, every `(defX …)`
    /// typed-domain call form, and every macroexpander template head
    /// takes on the outer [`Sexp`] algebra.
    ///
    /// Composition law (forward, through the outer algebra's atomic +
    /// residual construct families): `Sexp::call(head, args) ==
    /// Sexp::list(std::iter::once(Sexp::symbol(head)).chain(args))` for
    /// every `head: impl Into<String>` + `args: impl IntoIterator<Item
    /// = Sexp>`. The body binds through the SAME two construct methods
    /// consumers already reach for when threading a head-then-rest
    /// sequence into a call form — the composition law lifts that
    /// two-method inline pattern to ONE named query on the outer
    /// [`Sexp`] algebra.
    ///
    /// Round-trip law (section-for-retraction with the soft-projection
    /// sibling): for every `head: &str` + `args: Vec<Sexp>`,
    /// `Sexp::call(head, args.clone()).as_call() == Some((head,
    /// args.as_slice()))` — the outer algebra's call-form typed
    /// constructor pairs section-for-retraction with the outer
    /// algebra's soft call-form projection, and the (head symbol,
    /// args slice) cross-projection preserves identity. Keyword-
    /// matched round-trip law: for every `head: &str` + `args:
    /// Vec<Sexp>`, `Sexp::call(head, args.clone()).as_call_to(head) ==
    /// Some(args.as_slice())` — the keyword-typed projection recovers
    /// the args tail iff its argument keyword matches the constructor's
    /// head. Head-symbol composition law: `Sexp::call(head,
    /// args).head_symbol() == Some(head.as_str())` for every `head:
    /// impl Into<String>` + `args: impl IntoIterator<Item = Sexp>` —
    /// the head-position projection recovers the constructor's head
    /// byte-for-byte.
    ///
    /// Outer-shape composition law: `Sexp::call(head, args).shape() ==
    /// SexpShape::List` for every input — a call form is a list-shaped
    /// [`Sexp`], and the outer-shape identity binds through the typed-
    /// shape lattice at the residual arm. Structural-carving-marker
    /// composition law: `Sexp::call(head, args).as_structural_kind()
    /// == Some(StructuralKind::List)` — the residual-axis carving
    /// marker binds through the closed-set [`StructuralKind`] algebra
    /// at ONE arm, symmetric with the atomic-axis's `Sexp::X_atom(
    /// payload).as_atom_kind() == Some(AtomKind::X)` marker
    /// composition.
    ///
    /// Pre-lift the `Sexp::List(std::iter::once(Sexp::symbol(head))
    /// .chain(args).collect())` composition (or equivalently the
    /// `Sexp::List(vec![Sexp::symbol(head), args...])` welded triple)
    /// appeared inline at every consumer that builds a call-shaped
    /// [`Sexp`] value — well past the ≥2 PRIME-DIRECTIVE trigger once
    /// the call-form shape is named. Post-lift consumers that have a
    /// head string + an owned or iterable sequence of args bind to ONE
    /// typed-algebra method on the outer [`Sexp`] algebra with the
    /// `impl Into<String>` bound on the head absorbing `&str` /
    /// `String` / `&String` and the `impl IntoIterator<Item = Sexp>`
    /// bound on the args absorbing `Vec<Sexp>` / `[Sexp; N]` /
    /// `.map(...)` chains without a per-site `.collect::<Vec<Sexp>>()`
    /// coercion.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 1 — typed entry; the
    /// (head string, args sequence, [`Self::List`] tuple-variant
    /// constructor) triple binds at ONE typed-algebra method on the
    /// outer [`Sexp`] algebra, closing the call-form (construct,
    /// project) algebra dual pair with [`Self::as_call`] /
    /// [`Self::as_call_to`] / [`Self::head_symbol`]. THEORY.md §II.1
    /// invariant 2 — free middle; every consumer that has a head
    /// string + an owned or iterable sequence of args and wants to
    /// build a call-shaped [`Sexp`] routes through the SAME typed
    /// method, so a regression that drifts one consumer's construction
    /// from the others (e.g. a copy-edit that emits `Sexp::keyword(
    /// head)` for the head position, or that swaps in a `Sexp::string`
    /// head that [`Self::as_call`] then rejects at the projection
    /// site) cannot reach the substrate's runtime. THEORY.md §V.1 —
    /// knowable platform; the call-form typed-construct becomes a TYPE
    /// projection on the substrate's outer [`Sexp`] algebra sitting
    /// next to the typed-project family [`Self::as_call`] /
    /// [`Self::as_call_to`] rather than bare tuple-variant constructor
    /// paired with per-site `Sexp::List(vec![Sexp::symbol(...), ...])`
    /// discipline. THEORY.md §VI.1 — generation over composition; the
    /// call-form pair emerges from ONE typed-algebra composition
    /// through [`Self::list`] composed with [`Self::symbol`] rather
    /// than from per-consumer per-callsite literals; a future call-
    /// form shape extension (e.g. a keyword-headed call form for a
    /// Kernel-style applicative-vs-operative split) lands as ONE peer
    /// constructor on this algebra alongside the residual, quote-
    /// family, and atomic-payload construct families.
    #[must_use]
    pub fn call<H, I>(head: H, args: I) -> Self
    where
        H: Into<String>,
        I: IntoIterator<Item = Sexp>,
    {
        Self::list(std::iter::once(Self::symbol(head)).chain(args))
    }

    /// Canonical named-call-form outer constructor — composes the call-
    /// form typed constructor [`Self::call`] with the atomic-payload
    /// construct family's [`Self::symbol`] (for the NAME slot) via
    /// `std::iter::once(Self::symbol(name)).chain(spec_args)` to build a
    /// `(head NAME spec_args…)` symbol-headed named list-shaped [`Sexp`]
    /// value at ONE site on the closed-set [`Sexp`] algebra. The named-
    /// call-form section-for-retraction sibling of the existing
    /// [`Self::as_named_call_to`] soft-projection ([`Option<crate::error::
    /// Result<(&str, &[Sexp])>>`]): where the projection soft-decomposes
    /// a `(<keyword> NAME spec_args…)` symbol-headed list into its NAME
    /// symbol and spec args tail through the named-form gate
    /// ([`crate::compile::split_name_slot`]), this constructor embeds a
    /// fresh `(head string, name string, spec_args sequence)` triple
    /// into the matching named-call-shaped wrapper. Composition sibling
    /// of the call-form construct [`Self::call`] on the outer algebra:
    /// where [`Self::call`] closes the (construct, project) dual on the
    /// CALL-FORM TYPED DECOMPOSITION (`(head args…)`), this closes the
    /// dual on the NAMED-CALL-FORM TYPED DECOMPOSITION (`(head NAME
    /// spec_args…)`) — the load-bearing shape every `(defX NAME …)`
    /// typed-domain named authoring form takes on the outer [`Sexp`]
    /// algebra, and the section-for-retraction dual of the
    /// [`crate::compile::split_name_slot`] gate at the value level.
    ///
    /// Composition law (forward, through the call-form + atomic-payload
    /// construct families): `Sexp::named_call(head, name, spec_args) ==
    /// Sexp::call(head, std::iter::once(Sexp::symbol(name)).chain(
    /// spec_args))` for every `head: impl Into<String>` + `name: impl
    /// Into<String>` + `spec_args: impl IntoIterator<Item = Sexp>`. The
    /// body binds through the SAME two construct methods consumers
    /// already reach for when threading a head-then-name-then-rest
    /// sequence into a named call form — the composition law lifts that
    /// two-method inline pattern to ONE named query on the outer
    /// [`Sexp`] algebra.
    ///
    /// Round-trip law (section-for-retraction with the named-form soft-
    /// projection): for every `head: &'static str` + `name: &str` +
    /// `spec_args: Vec<Sexp>`, `Sexp::named_call(head, name, spec_args
    /// .clone()).as_named_call_to(head) == Some(Ok((name, spec_args
    /// .as_slice())))` — the outer algebra's named-call-form typed
    /// constructor pairs section-for-retraction with the outer
    /// algebra's soft named-call-form projection, and the (head symbol,
    /// NAME symbol, spec args slice) cross-projection preserves
    /// identity. Call-form projection composition: `Sexp::named_call(
    /// head, name, spec_args).as_call() == Some((head,
    /// once(Sexp::symbol(name)).chain(spec_args).collect().as_slice()
    /// ))` — the call-form soft-projection recovers `(head, [name,
    /// spec_args…])` with the NAME symbol as the first arg, mirroring
    /// the [`Self::call`] round-trip on the encompassing call algebra.
    /// Keyword-matched round-trip law: for every `head: &'static str` +
    /// `name: &str` + `spec_args: Vec<Sexp>`, `Sexp::named_call(head,
    /// name, spec_args.clone()).as_call_to(head) == Some(
    /// [Sexp::symbol(name), spec_args…].as_slice())` — the keyword-
    /// typed projection recovers the NAME-headed args tail iff its
    /// argument keyword matches the constructor's head. Head-symbol
    /// composition law: `Sexp::named_call(head, name, spec_args)
    /// .head_symbol() == Some(head.as_str())` — the head-position
    /// projection recovers the constructor's head byte-for-byte.
    ///
    /// Outer-shape composition law: `Sexp::named_call(head, name,
    /// spec_args).shape() == SexpShape::List` for every input — a
    /// named call form is a list-shaped [`Sexp`], the outer-shape
    /// identity binds through the typed-shape lattice at the residual
    /// arm. Structural-carving-marker composition law: `Sexp::
    /// named_call(head, name, spec_args).as_structural_kind() ==
    /// Some(StructuralKind::List)` — the residual-axis carving marker
    /// binds through the closed-set [`StructuralKind`] algebra at ONE
    /// arm, symmetric with [`Self::call`]'s residual-arm marker
    /// composition.
    ///
    /// Named-form gate composition law: `crate::compile::split_name_slot(
    /// &Sexp::named_call(head, name, spec_args).as_call_to(head)
    /// .unwrap(), head) == Ok((name, spec_args.as_slice()))` — the
    /// substrate's named-form arity + NAME-shape gate accepts every
    /// output of this constructor byte-for-byte, closing the section-
    /// for-retraction pair at the gate level as well as at the
    /// projection level. A constructor emission that drifts into a
    /// missing-NAME shape (empty spec_args yields `(head)`, which the
    /// call-form projection recovers but the named-form gate rejects
    /// with `NamedFormMissingName`) or a non-symbol-NAME shape
    /// (`Sexp::keyword(name)` for the NAME position, which the gate
    /// rejects with `NamedFormNonSymbolName`) becomes structurally
    /// impossible — the `impl Into<String>` NAME bound admits string
    /// payloads only, and the [`Self::symbol`] wrap routes to the
    /// symbol atom variant `as_symbol_or_string` accepts.
    ///
    /// Pre-lift the `Sexp::call(head, std::iter::once(Sexp::symbol(
    /// name)).chain(spec_args))` composition (or equivalently the
    /// `Sexp::List(vec![Sexp::symbol(head), Sexp::symbol(name),
    /// spec_args...])` welded quadruple) appeared inline at every
    /// consumer that builds a `(defX NAME …)`-shaped [`Sexp`] value
    /// — well past the ≥2 PRIME-DIRECTIVE trigger once the named
    /// call-form shape is named. Post-lift consumers that have a head
    /// string + a NAME string + an owned or iterable sequence of spec
    /// args bind to ONE typed-algebra method on the outer [`Sexp`]
    /// algebra with the two `impl Into<String>` bounds absorbing `&str`
    /// / `String` / `&String` on both string positions and the
    /// `impl IntoIterator<Item = Sexp>` bound on the spec args
    /// absorbing `Vec<Sexp>` / `[Sexp; N]` / `.map(...)` chains without
    /// a per-site `.collect::<Vec<Sexp>>()` coercion.
    ///
    /// Frontier inspiration: Racket's `syntax-parse`
    /// `(~datum keyword) name:id spec ...` pattern binds the NAME slot
    /// through the `name:id` capture binder and consumers reference it
    /// downstream; the constructor peer on the same surface is
    /// `syntax-e` composed with `datum->syntax` wrapping a
    /// `(list #'keyword name-id spec-list ...)` triple. `Sexp::
    /// named_call` is the unstructured-Rust peer — a section-for-
    /// retraction constructor on the outer algebra that mirrors the
    /// `~datum keyword name:id spec ...` pattern's NAME capture on the
    /// construct side. Tree-sitter's `query`-matched named captures
    /// have the same shape on the tree side: the query pattern
    /// binds a NAME capture, the constructor peer (`ts_node_new`
    /// composed with `ts_node_field_set`) embeds a fresh NAME child at
    /// the corresponding field slot. The typed structural rejection
    /// chain the substrate's named-form gate emits
    /// ([`crate::error::LispError::NamedFormMissingName`],
    /// [`crate::error::LispError::NamedFormNonSymbolName`]) is
    /// preserved by construction — the constructor cannot emit a
    /// value the gate rejects, symmetric with the `~datum` /
    /// `name:id` reader-side rejection that fires BEFORE any
    /// downstream binding sees the drifted shape.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 1 — typed entry; the
    /// (head string, NAME string, spec args sequence, [`Self::call`]
    /// call-form constructor) quadruple binds at ONE typed-algebra
    /// method on the outer [`Sexp`] algebra, closing the named-call-
    /// form (construct, project) algebra dual pair with
    /// [`Self::as_named_call_to`] / [`Self::as_named_call_to_any`] on
    /// the projection side and [`crate::compile::split_name_slot`] on
    /// the gate side. THEORY.md §II.1 invariant 2 — free middle;
    /// every consumer that has a head + NAME + spec args and wants to
    /// build a named-call-shaped [`Sexp`] routes through the SAME
    /// typed method, so a regression that drifts one consumer's
    /// construction from the others (e.g. a copy-edit that emits
    /// `Sexp::keyword(name)` for the NAME position, which the
    /// named-form gate would reject with `NamedFormNonSymbolName`)
    /// cannot reach the substrate's runtime. THEORY.md §V.1 —
    /// knowable platform; the named-call-form typed-construct becomes
    /// a TYPE projection on the substrate's outer [`Sexp`] algebra
    /// sitting next to the typed-project family [`Self::
    /// as_named_call_to`] / [`Self::as_named_call_to_any`] +
    /// [`crate::ast::iter_named_calls_to`] /
    /// [`crate::ast::iter_named_calls_to_any`] rather than a per-site
    /// inline composition. THEORY.md §VI.1 — generation over
    /// composition; the named-call-form pair emerges from ONE typed-
    /// algebra composition through [`Self::call`] composed with
    /// [`Self::symbol`] rather than from per-consumer per-callsite
    /// literals; a future named-form shape extension (e.g. a
    /// dotted-NAME form, or a typed-NAME form where the NAME slot
    /// carries a compile-time-decoded typed witness) lands as ONE
    /// peer constructor on this algebra alongside the call-form,
    /// residual, quote-family, and atomic-payload construct
    /// families.
    #[must_use]
    pub fn named_call<H, N, I>(head: H, name: N, spec_args: I) -> Self
    where
        H: Into<String>,
        N: Into<String>,
        I: IntoIterator<Item = Sexp>,
    {
        Self::call(head, std::iter::once(Self::symbol(name)).chain(spec_args))
    }

    /// Decompose a call form into its argument tail IFF the head matches the
    /// supplied `keyword` — `Some(args)` iff this is a non-empty list whose
    /// first element is a symbol equal to `keyword`, where `args` is the
    /// remaining elements (`&self[1..]`, possibly empty). `None` for every
    /// shape `as_call` rejects AND for every call whose head is present but
    /// differs from `keyword`.
    ///
    /// This is the *keyword-typed call decomposition* — the natural
    /// extension of [`Sexp::as_call`] for the "is this a call to ONE
    /// specific operator?" question every typed-domain dispatch site asks
    /// after macroexpansion. [`compile_typed`](crate::compile::compile_typed)
    /// and [`compile_named_from_forms`](crate::compile::compile_named_from_forms)
    /// both opened the same two-step chain inline —
    /// `if let Some((head, args)) = form.as_call() { if head == T::KEYWORD { … } }`
    /// — at every form they walked; the chain IS this projection. Naming
    /// it lifts "is this form a call to T?" from a two-step inline pattern
    /// to ONE structural query on the `Sexp` algebra. A regression that
    /// drifts one consumer's comparison from `==` to `!= `, or that
    /// compares against a different label than `T::KEYWORD` (e.g.
    /// substring-grepping the rendered head), becomes structurally
    /// impossible: there is exactly one implementation both dispatchers
    /// route through.
    ///
    /// Soft face, like the rest of the `as_*` family: it answers "is this
    /// a call to `keyword`, and what are its arguments?" and yields `None`
    /// for everything that isn't (skip / fall through), with no
    /// diagnostic. The strict gate sibling is
    /// `TataraDomain::compile_from_sexp`, which distinguishes the
    /// not-a-list / empty-list / non-symbol-head / wrong-keyword
    /// sub-modes to reject loudly. The two are the dispatch
    /// (`as_call_to`) and the gate (`compile_from_sexp`) faces of the
    /// same projection; keeping both lets a site choose "skip silently"
    /// or "reject loudly" without re-deriving the head.
    ///
    /// Structural identity binding it to its siblings:
    ///   * `as_call_to(keyword) == as_call().and_then(|(h, args)| (h == keyword).then_some(args))`
    ///   * `as_call_to(keyword).is_some() == (head_symbol() == Some(keyword))`
    ///
    /// The returned `&[Sexp]` borrows from the list's tail verbatim — no
    /// copy, no allocation, same lifetime as [`Sexp::as_call`]'s tail.
    ///
    /// Slice-side sibling: [`iter_calls_to`] lifts this per-form projection
    /// onto a `&[Sexp]`, yielding the args slices of every matching form in
    /// source order — the substrate's typed-keyword filter over a batch of
    /// forms, structurally bound to this per-form projection via the
    /// closed-form composition
    /// `iter_calls_to(forms, k) == forms.iter().filter_map(|f| f.as_call_to(k))`.
    pub fn as_call_to(&self, keyword: &str) -> Option<&[Sexp]> {
        let (head, args) = self.as_call()?;
        (head == keyword).then_some(args)
    }

    /// Decompose a call form whose head decodes through a caller-supplied
    /// classifier — `Some((decoded, args))` iff this is a non-empty list
    /// whose first element is a symbol AND `decode(head)` returns
    /// `Some(decoded)`, where `args` is the remaining elements
    /// (`&self[1..]`, possibly empty). `None` for every shape
    /// [`Sexp::as_call`] rejects AND for every call whose head is present
    /// but `decode` rejects.
    ///
    /// This is the *typed-decoded call decomposition* — the closure-typed
    /// extension of [`Sexp::as_call_to`] for the "is this a call whose head
    /// belongs to a CLOSED SET (or a LIVE REGISTRY) that decodes to a typed
    /// witness?" question. Where [`Sexp::as_call_to`] filters by ONE
    /// constant keyword, `as_call_to_any` filters AND TYPES by a caller-
    /// supplied projection — every dispatch site that asks "is this form
    /// an invocation of any of N operators, decoded as a typed enum or
    /// resolved against a runtime table?" binds to ONE structural query
    /// on the `Sexp` algebra. Two consumers route through it:
    ///
    ///   * The macro-expander's `macro_def_from` — closed-set classifier:
    ///     `as_call_to_any(MacroDefHead::from_keyword)` decides which of
    ///     `{defmacro, defpoint-template, defcheck}` a top-level form
    ///     invokes, decoded to the typed `MacroDefHead` enum. Pre-lift the
    ///     site opened the same three-step chain inline — `let Some(list)
    ///     = form.as_list()…; let Some(head) = form.head_symbol()…; let
    ///     Some(decoded) = MacroDefHead::from_keyword(head)…`.
    ///   * The macro-expander's `Expander::expand` — live-registry
    ///     classifier: `as_call_to_any(|h| self.macros.get(h))` decides
    ///     which of the registered macros (a `HashMap<String, MacroDef>`
    ///     populated by `expand_program`'s `defmacro` recognition) a form
    ///     invokes, decoded to `&MacroDef`. Pre-lift the site opened the
    ///     same `as_list() + as_call() + self.macros.get(head)` chain
    ///     inline — `as_list()` for the children-walk fallthrough,
    ///     `as_call()` for the (head, args) pair (which itself re-derives
    ///     `as_list()` internally), and `self.macros.get(head)` for the
    ///     registry lookup.
    ///
    /// Naming the projection lifts "is this form a call to any of N
    /// operators, decoded to T?" from the three-step inline pattern to
    /// ONE structural query — closed-set enum classifier OR live-registry
    /// HashMap classifier, the family primitive is uniform under both.
    ///
    /// Soft face, like the rest of the `as_*` family: it answers "is this
    /// a call whose head decodes through `F`, and what are its arguments?"
    /// and yields `None` for everything that isn't (skip / fall through),
    /// with no diagnostic. The strict gate sibling stays
    /// `TataraDomain::compile_from_sexp` — that distinguishes the
    /// not-a-list / empty-list / non-symbol-head / wrong-keyword sub-modes
    /// to reject loudly for a single-keyword consumer. The two are the
    /// closed-set-decoded dispatch (`as_call_to_any`) and the
    /// single-keyword gate (`compile_from_sexp`) faces of the typed-domain
    /// recognition problem; keeping both lets a site choose "skip
    /// silently if the head isn't ours" or "reject loudly if the head
    /// isn't the exact keyword" without re-deriving the head.
    ///
    /// Structural identity binding it to its siblings:
    ///   * `as_call_to_any(decode) == as_call().and_then(|(h, args)| decode(h).map(|d| (d, args)))`
    ///   * `as_call_to(k) == as_call_to_any(|h| (h == k).then_some(())).map(|(_, a)| a)` (modulo the discarded `()`)
    ///   * `as_call_to_any(decode).is_some() == as_call().map_or(false, |(h, _)| decode(h).is_some())`
    ///
    /// The returned `&[Sexp]` borrows from the list's tail verbatim — no
    /// copy, no allocation, same lifetime as [`Sexp::as_call`]'s tail.
    /// `T` is owned because `decode` is `FnOnce(&str) -> Option<T>` and a
    /// `&'_ str` borrow into the head symbol would not outlive the helper
    /// boundary; consumers projecting to a typed `Copy` enum (e.g.
    /// `MacroDefHead`) get the value directly, consumers projecting to a
    /// borrowed `&'static str` (a closed-set head) project to
    /// `&'static str` and inherit the static lifetime through the
    /// classifier.
    ///
    /// Slice-side sibling: [`iter_calls_to_any`] lifts this per-form
    /// projection onto a `&[Sexp]`, yielding the `(decoded, &[Sexp])`
    /// pair of every matching form in source order — the substrate's
    /// typed-decoded filter over a batch of forms, structurally bound
    /// to this per-form projection via the closed-form composition
    /// `iter_calls_to_any(forms, decode) == forms.iter().filter_map(|f|
    /// f.as_call_to_any(&mut decode))`. The slice-side primitive
    /// promotes the closure constraint from [`FnOnce`] (per-form, one
    /// call per invocation) to [`FnMut`] (slice-side, one call per
    /// element) so a decoder that captures mutable state (a counter, a
    /// registry cache) maintains state across the batch walk.
    pub fn as_call_to_any<F, T>(&self, decode: F) -> Option<(T, &[Sexp])>
    where
        F: FnOnce(&str) -> Option<T>,
    {
        let (head, args) = self.as_call()?;
        decode(head).map(|d| (d, args))
    }

    /// Decompose a named call form (a `(<keyword> NAME :k v …)` shape) whose
    /// head decodes through a caller-supplied classifier — `Some(Ok((decoded,
    /// name, spec_args)))` iff this is a non-empty list whose first element
    /// is a symbol AND `decode(head)` returns `Some((decoded, kw))` AND the
    /// remaining elements split cleanly into a NAME slot (symbol or string
    /// at position 1) and a spec args tail (position 2..), `Some(Err(…))` iff
    /// the head decodes but the NAME slot is missing
    /// ([`LispError::NamedFormMissingName`]) or non-symbol-or-string
    /// ([`LispError::NamedFormNonSymbolName`]), `None` for every shape
    /// [`Sexp::as_call_to_any`] rejects AND for every call whose head is
    /// present but `decode` returns `None` for.
    ///
    /// This is the *per-form named-classifier projection* — the per-form
    /// peer of [`iter_named_calls_to_any`] on the slice algebra and of
    /// [`crate::macro_expand::Expander::expand_and_collect_named_calls_to_any`]
    /// on the expander surface. Closes the (per-form × classifier × named)
    /// corner of the soft-dispatch cube the substrate's per-form algebra
    /// (`as_call_to{,_any}`) and slice algebra (`iter_calls_to{,_any}`,
    /// `iter_named_calls_to{,_any}`) collectively shape — pre-lift the cube
    /// at the per-form × named corner was "(composed inline at each named
    /// consumer)" (the documented gap the cube table inside
    /// [`iter_named_calls_to_any`] called out), post-lift the per-form ×
    /// named row binds to ONE primitive every per-form named consumer
    /// composes through:
    ///
    /// |                | bare-kwargs                  | named NAME-then-kwargs               |
    /// |----------------|------------------------------|--------------------------------------|
    /// | per-form       | [`Sexp::as_call_to_any`]     | `as_named_call_to_any` (this)        |
    /// | slice          | [`iter_calls_to_any`]        | [`iter_named_calls_to_any`]          |
    /// | expander       | `expand_and_collect_calls_to_any` | `expand_and_collect_named_calls_to_any` |
    ///
    /// The slice-side [`iter_named_calls_to_any`] now routes through THIS
    /// per-form primitive via the SAME `forms.iter().filter_map(_)`
    /// skeleton [`iter_calls_to_any`] uses to route through
    /// [`Sexp::as_call_to_any`], so a regression that drifts ONE row's
    /// instrumentation, span-aware borrow walker, or fused-iterator
    /// invariant from the bare row to the named row (or vice versa) is
    /// structurally impossible.
    ///
    /// Composes [`Sexp::as_call_to_any`] with
    /// [`crate::compile::split_name_slot`]: the classifier filter precedes
    /// the named gate, mirroring how `split_name_slot` is composed AFTER
    /// the classifier-decoded args tail is already in hand inside
    /// [`iter_named_calls_to_any`]. Decoder signature `FnOnce(&str) ->
    /// Option<(T, &'static str)>` pairs the typed witness `T` with the
    /// canonical static keyword threaded through the
    /// `NamedFormMissingName.keyword` / `NamedFormNonSymbolName.keyword`
    /// slots of the named-form gate — the `&'static` constraint pins the
    /// same compile-time discipline [`crate::compile::split_name_slot`]'s
    /// `keyword: &'static str` parameter pins at the slice-side boundary,
    /// AND that the slice-side decoder signature pins on the slice
    /// algebra.
    ///
    /// Three-arm result shape — `Option<Result<…>>` — preserves both the
    /// classifier filter face (`None` for "not our head, skip silently",
    /// matching the per-form soft-projection posture of every other `as_*`
    /// method on `Sexp`) AND the named gate face (`Err` for "matched head
    /// but malformed NAME", surfacing the typed structural-rejection
    /// variants `LispError::NamedFormMissingName` /
    /// `LispError::NamedFormNonSymbolName` the slice-side and expander-
    /// surface consumers already short-circuit on). A consumer that wants
    /// "fold over every per-form result, short-circuiting on the first
    /// malformed NAME" composes `.transpose()` (yielding
    /// `Result<Option<…>>`) and `?`-routes the outer `Result`; a consumer
    /// that wants "skip every non-matching form AND every malformed
    /// matched form" composes `.and_then(|res| res.ok())` (yielding
    /// `Option<(T, &str, &[Sexp])>`); a consumer that wants the raw
    /// three-arm shape pattern-matches directly.
    ///
    /// Two plausible future consumer shapes the per-form named-classifier
    /// projection admits with no boilerplate:
    ///   * **LSP hover tooltip** — an authoring tool that surfaces a
    ///     tooltip on the symbol under the cursor wants to ask "is THIS
    ///     form (the one I just resolved to under the cursor) a named
    ///     call to any registered domain, decoded to a typed kind, with
    ///     the borrowed NAME slot extracted for the tooltip body?". Pre-
    ///     lift the tool would re-derive `form.as_call_to_any(decode)
    ///     .and_then(|((kind, kw), args)| split_name_slot(args,
    ///     kw).ok().map(|(name, rest)| (kind, name, rest)))` inline;
    ///     post-lift the tool binds to ONE primitive.
    ///   * **REPL single-form dispatcher** — a `:dispatch <classifier>
    ///     <form>` command that walks a single form through the
    ///     registry classifier, reporting the typed kind AND the NAME
    ///     slot (for "you said `(defmonitor my-monitor …)`, I see
    ///     `Monitor` named `my-monitor` with 3 spec args"). Pre-lift
    ///     the REPL would re-derive the same inline composition; post-
    ///     lift the REPL binds to ONE primitive, sibling shape to how
    ///     [`Sexp::as_call_to_any`] backs the slice-side dispatcher
    ///     [`iter_calls_to_any`].
    ///
    /// Structural identity binding it to its siblings:
    ///   * `as_named_call_to_any(decode) == as_call_to_any(decode).map(|((d, kw), args)| split_name_slot(args, kw).map(|(name, rest)| (d, name, rest)))`
    ///   * `as_named_call_to(k) == as_named_call_to_any(|h| (h == k).then_some(((), k))).map(|res| res.map(|(_, name, rest)| (name, rest)))`
    ///   * `as_named_call_to_any(decode).is_none() == as_call_to_any(decode).is_none()` (the classifier filter face is identical to the bare-kwargs sibling's)
    ///
    /// The returned `&str` NAME slot and `&[Sexp]` spec args tail borrow
    /// from `&self` verbatim — no copy, no allocation, same lifetime as
    /// [`Sexp::as_call_to_any`]'s tail AND [`crate::compile::split_name_slot`]'s
    /// pair. `T` is owned because the underlying [`Sexp::as_call_to_any`]
    /// classifier is `FnOnce(&str) -> Option<(T, &'static str)>` and `T`
    /// must outlive the helper boundary; consumers projecting to a typed
    /// `Copy` enum (e.g. a closed-set `Kind`) get the value directly,
    /// consumers projecting to a borrowed `&'static str` (a closed-set
    /// head sourced from `ClosedSet::ALL.label()`) project to `&'static
    /// str` and inherit the static lifetime through the classifier.
    ///
    /// Soft face on the classifier filter, strict face on the named gate:
    /// "is this a named call whose head decodes through `F`, and what
    /// are its NAME and spec args?" yielding `None` for "not our head"
    /// (skip / fall through, no diagnostic) AND `Some(Err(…))` for "our
    /// head but malformed NAME" (reject loudly, structural variant). The
    /// soft-classifier-then-strict-named composition matches the
    /// slice-side `iter_named_calls_to_any` yielded `Result` shape (with
    /// non-matching forms skipped by the iterator filter) and the
    /// expander-surface `expand_and_collect_named_calls_to_any` collect
    /// shape (with `Result::collect` short-circuiting on the first
    /// malformed NAME) — every layer of the cube preserves both faces.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition; the
    /// per-form × classifier × named cell of the soft-dispatch cube is a
    /// CONSEQUENCE of [`Sexp::as_call_to_any`] + [`crate::compile::split_name_slot`],
    /// named on the substrate's `Sexp` algebra rather than re-derived
    /// inline at every per-form named consumer site. THEORY.md §V.1 —
    /// knowable platform; the per-form named-classifier projection
    /// becomes a NAMED primitive on the `Sexp` algebra, discoverable by
    /// any future authoring tool (LSP, REPL, `tatara-check`) that holds
    /// a single form in isolation. THEORY.md §II.1 invariant 2 — free
    /// middle; the slice-side sibling [`iter_named_calls_to_any`] now
    /// routes through this per-form primitive via the same
    /// `forms.iter().filter_map(_)` skeleton the bare-kwargs row uses
    /// to route through [`Sexp::as_call_to_any`], so the bare and named
    /// rows share ONE filter-and-fuse implementation skeleton on the
    /// `Sexp`/`&[Sexp]` algebras.
    ///
    /// Frontier inspiration: MLIR's `mlir::dyn_cast<NamedOpInterface>(op)`
    /// — the typed downcast from a polymorphic IR node onto a NAMED-op
    /// interface that exposes both the typed witness AND the
    /// symbol-name accessor is the MLIR idiom; `as_named_call_to_any` is
    /// the unstructured-Rust peer on the substrate's `Sexp` algebra,
    /// with `Option<Result<(T, &str, &[Sexp])>>` standing in for MLIR's
    /// typed-downcast-then-name-accessor pair, and the `Result` face
    /// carrying the typed structural rejection MLIR encodes via verifier
    /// diagnostics. Racket's `syntax-parse` `~or* ((~datum defX) name:id
    /// arg ...) ((~datum defY) name:id arg ...)` on a single syntax
    /// object — typed named-form decomposition with `name:id` capture
    /// binding is the Racket idiom; this method is the per-form
    /// Rust-typed peer with the typed structural rejection
    /// (`NamedFormMissingName` / `NamedFormNonSymbolName`) preserved
    /// across the boundary.
    pub fn as_named_call_to_any<F, T>(
        &self,
        decode: F,
    ) -> Option<crate::error::Result<(T, &str, &[Sexp])>>
    where
        F: FnOnce(&str) -> Option<(T, &'static str)>,
    {
        self.as_call_to_any(decode).map(|((decoded, kw), args)| {
            let (name, spec_args) = crate::compile::split_name_slot(args, kw)?;
            Ok((decoded, name, spec_args))
        })
    }

    /// Decompose a named call form whose head matches a constant
    /// `keyword` — `Some(Ok((name, spec_args)))` iff this is a non-empty
    /// list whose first element is the symbol `keyword` AND the remaining
    /// elements split cleanly into a NAME slot and a spec args tail,
    /// `Some(Err(…))` iff the head matches but the NAME slot is missing
    /// or non-symbol-or-string, `None` for every shape
    /// [`Sexp::as_call_to`] rejects.
    ///
    /// Constant-keyword sibling of [`Sexp::as_named_call_to_any`] and
    /// per-form sibling of [`iter_named_calls_to`] on the slice algebra.
    /// Routes through the typed-decoded sibling with a constant-classifier
    /// decoder (`|h| (h == keyword).then_some(((), keyword))`) — the same
    /// constant-classifier composition [`Sexp::as_call_to`] uses to route
    /// through [`Sexp::as_call_to_any`] on the bare-kwargs axis, and that
    /// [`iter_named_calls_to`] uses to route through
    /// [`iter_named_calls_to_any`] on the slice algebra. The discarded
    /// `()` typed witness (`then_some(((), keyword))`) is consumed by the
    /// wrapper projection so the consumer's per-form mapper sees only the
    /// `(name, spec_args)` borrowed pair, matching the bare projection
    /// signature on the named axis.
    ///
    /// `keyword: &'static str` threads verbatim through the
    /// `NamedFormMissingName.keyword` / `NamedFormNonSymbolName.keyword`
    /// slots of the named-form gate — same `&'static` discipline
    /// [`crate::compile::split_name_slot`] pins at its boundary, AND that
    /// [`iter_named_calls_to`] pins on the slice algebra. Consumers that
    /// want a runtime keyword whose lifetime is shorter use
    /// [`Sexp::as_named_call_to_any`] directly with a constant-classifier
    /// decoder that converts post-resolution.
    ///
    /// Structural identity binding it to its siblings:
    ///   * `as_named_call_to(k) == as_named_call_to_any(|h| (h == k).then_some(((), k))).map(|res| res.map(|(_, name, rest)| (name, rest)))`
    ///   * `as_named_call_to(k).is_none() == as_call_to(k).is_none()`
    ///   * `iter_named_calls_to(forms, k) == forms.iter().filter_map(|f| f.as_named_call_to(k))`
    ///
    /// Theory anchor: see [`Sexp::as_named_call_to_any`] — the constant-
    /// keyword sibling shares the same lift posture, threading the
    /// `&'static str` keyword constraint through the named-form gate's
    /// canonical-keyword slot rather than admitting an arbitrary runtime
    /// keyword.
    pub fn as_named_call_to(
        &self,
        keyword: &'static str,
    ) -> Option<crate::error::Result<(&str, &[Sexp])>> {
        self.as_named_call_to_any(move |h| (h == keyword).then_some(((), keyword)))
            .map(|res| res.map(|(_, name, rest)| (name, rest)))
    }

    /// Decompose an unquote-family form into its typed marker and inner
    /// expression — `Some((UnquoteForm::Unquote, inner))` iff this is `,x`
    /// (a [`Sexp::Unquote`] wrapper), `Some((UnquoteForm::Splice, inner))`
    /// iff this is `,@x` (a [`Sexp::UnquoteSplice`] wrapper), `None` for
    /// every other shape (Quote, Quasiquote, Nil, Atom, List).
    ///
    /// This is the *unquote-family projection* — the typed-marker peer of
    /// [`Sexp::as_call`] for the macro-template substitution surface. Where
    /// [`Sexp::as_call`] decomposes `(op args …)` into a `(head, args)`
    /// pair, `as_unquote` decomposes `,x` / `,@x` into a `(form, inner)`
    /// pair where `form: UnquoteForm` is the closed-set typed marker
    /// (`Unquote` for `,`, `Splice` for `,@`) and `inner: &Sexp` is the
    /// borrowed body. The pairing of `Sexp::Unquote ↔ UnquoteForm::Unquote`
    /// and `Sexp::UnquoteSplice ↔ UnquoteForm::Splice` is the structural
    /// invariant the macro-expander's substitution path keys every
    /// rejection on — naming the projection lifts the pair from
    /// per-callsite discipline (two `Sexp::Unquote(inner)` arms paired
    /// with two `UnquoteForm::Unquote` literals at distinct sites, two
    /// `Sexp::UnquoteSplice(inner)` arms paired with two
    /// `UnquoteForm::Splice` literals at distinct sites) into ONE typed
    /// projection both expansion strategies route through.
    ///
    /// Three consumers in [`macro_expand`](crate::macro_expand) route
    /// through this primitive:
    ///   * `compile_node` (bytecode-template compile path) — `,x` becomes
    ///     `TemplateOp::Subst(idx)`, `,@x` becomes `TemplateOp::Splice(idx)`;
    ///     both arms share the gate-1+gate-2 composition
    ///     `resolve_unquote_in_params(inner, params, form)?` keyed on the
    ///     typed `form` projection.
    ///   * `substitute` top-level (substitute fallback path) — `,x` resolves
    ///     to its bound value, `,@x` rejects with
    ///     `LispError::SpliceOutsideList` (a splice form with no containing
    ///     list to flatten into).
    ///   * `substitute` list-inner (substitute fallback path's per-item
    ///     walk) — `,@x` items splice their bound list/nil/scalar value
    ///     into the assembled list builder via
    ///     [`crate::macro_expand::splice_value_into`]; non-splice items
    ///     recurse into `substitute`.
    ///
    /// Pre-lift each site opened the same per-variant match arms —
    /// `Sexp::Unquote(inner) => … UnquoteForm::Unquote …` and
    /// `Sexp::UnquoteSplice(inner) => … UnquoteForm::Splice …` —
    /// independently. The (Sexp variant, UnquoteForm variant) pairing was
    /// load-bearing across distinct sites yet only enforced by callsite
    /// discipline. Post-lift the pair binds at ONE projection function the
    /// type system threads through `(UnquoteForm, &Sexp)`: a regression
    /// that drifts ONE site's pairing (e.g. a future emitter that matches
    /// `Sexp::Unquote(_)` but threads `UnquoteForm::Splice` into
    /// `unquote_target_symbol` — type-checks but renders a misleading
    /// diagnostic) becomes structurally impossible.
    ///
    /// Soft face, like the rest of the `as_*` family on `Sexp`: it answers
    /// "is this form an unquote-family marker, and what does it wrap?" and
    /// yields `None` for everything that isn't (skip / fall through), with
    /// no diagnostic. The strict siblings —
    /// [`crate::macro_expand::splice_value_into`] for the bound-list
    /// coercion, `non_symbol_unquote_target` /
    /// `splice_outside_list` for the per-failure-mode rejections — keep
    /// their loud-reject posture; this projection is the dispatch face the
    /// soft pre-rejection walk binds to.
    ///
    /// Structural identity binding it to the unquote-family variants:
    ///   * `as_unquote() == Some((UnquoteForm::Unquote, inner))` iff `self == Sexp::Unquote(inner)`
    ///   * `as_unquote() == Some((UnquoteForm::Splice, inner))`  iff `self == Sexp::UnquoteSplice(inner)`
    ///   * `as_unquote().is_some() == matches!(self, Sexp::Unquote(_) | Sexp::UnquoteSplice(_))`
    ///
    /// The returned `&Sexp` borrows the inner box's body verbatim — no
    /// clone, no allocation — same lifetime as `&self`. The closed-set
    /// guarantee on [`UnquoteForm`] (exactly `Unquote ⊎ Splice`) is
    /// threaded through this projection's return tuple, so consumers that
    /// pattern-match on `form: UnquoteForm` get rustc-enforced
    /// exhaustiveness — a future `Sexp` variant must extend `UnquoteForm`
    /// AND this match arm together (or stay outside the unquote family
    /// and project to `None`), eliminating the silent two-site
    /// extension-drift this lift was already designed to forbid.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition; the
    /// `(Sexp::Unquote, UnquoteForm::Unquote)` and
    /// `(Sexp::UnquoteSplice, UnquoteForm::Splice)` pairings appear ≥3
    /// times across `compile_node` (2 arms) + `substitute` (top-level +
    /// list-inner) — past the PRIME-DIRECTIVE trigger once the structural
    /// shape is named. THEORY.md §V.1 — knowable platform; the
    /// unquote-family projection becomes a NAMED primitive on the
    /// substrate's `Sexp` algebra rather than per-site `Sexp::Unquote(_)
    /// | Sexp::UnquoteSplice(_)` inline matches paired with per-site
    /// `UnquoteForm::Unquote` / `UnquoteForm::Splice` literals.
    /// THEORY.md §II.1 invariant 1 — typed entry; the macro-template
    /// substitution surface's typed-marker projection IS the rust-level
    /// typed-entry gate's structural component, lifted from per-site
    /// duplication onto ONE rust method the substrate's diagnostic
    /// promotions hang off of. THEORY.md §II.1 invariant 2 — free middle;
    /// both expansion strategies (bytecode `compile_node` and substitute
    /// fallback `substitute`) route through the SAME projection, so a
    /// regression that drifts ONE strategy's (Sexp variant, UnquoteForm
    /// variant) pairing from the other cannot reach the substrate's
    /// runtime — the type system binds both strategies to the
    /// projection's single emission shape.
    ///
    /// Frontier inspiration: Racket's `syntax-parse` `~or* (~unquote stx)
    /// (~unquote-splice stx)` pattern — every macro-template pattern over
    /// `,id` / `,@id` binds to ONE typed decomposition that surfaces the
    /// marker identity alongside the inner expression; the substrate's
    /// `as_unquote` is the Rust-typed peer of that pattern, lifted onto
    /// the `Sexp` algebra with [`UnquoteForm`] standing in for Racket's
    /// pattern-class identity. MLIR's typed-IR projection
    /// `mlir::dyn_cast<UnquoteFamilyOp>(op)` — the typed downcast from a
    /// polymorphic IR node onto a closed-set op family is the MLIR idiom;
    /// `as_unquote` is the unstructured-projection peer on the substrate's
    /// `Sexp` algebra, with `Option<(UnquoteForm, &Sexp)>` standing in for
    /// MLIR's typed downcast result.
    pub fn as_unquote(&self) -> Option<(UnquoteForm, &Sexp)> {
        let (qf, inner) = self.as_quote_form()?;
        qf.as_unquote_form().map(|uf| (uf, inner))
    }

    /// Soft projection onto the closed-set [`UnquoteForm`] template-
    /// substitution carving marker — the 2-of-12 carving of the
    /// [`SexpShape`](crate::error::SexpShape) algebra covering the two
    /// homoiconic template-substitution wrappers ([`Self::Unquote`] and
    /// [`Self::UnquoteSplice`]), which is itself a 2-of-4 subset of the
    /// quote-family carving ([`QuoteForm`]). Returns
    /// `Some(UnquoteForm::Unquote)` iff this is `,x` (a [`Self::Unquote`]
    /// wrapper), `Some(UnquoteForm::Splice)` iff this is `,@x` (a
    /// [`Self::UnquoteSplice`] wrapper), `None` for every other outer
    /// shape ([`Self::Nil`], every [`Self::Atom`] variant, [`Self::List`],
    /// and the two non-substitution quote-family wrappers [`Self::Quote`]
    /// and [`Self::Quasiquote`]).
    ///
    /// Direct value-level peer of the shape-level projection
    /// [`SexpShape::as_unquote_form`](crate::error::SexpShape::as_unquote_form)
    /// — the pair `(Sexp::as_unquote_form, SexpShape::as_unquote_form)`
    /// binds the (Sexp value, UnquoteForm carving marker) pairing at ONE
    /// typed method on each algebra, closing the unquote-subset cell of
    /// the (Sexp value → carving marker) matrix. Marker-only sibling of
    /// [`Self::as_unquote`] (which returns
    /// `Option<(UnquoteForm, &Sexp)>` — marker + wrapped inner) and
    /// direct 2-of-4 subset peer of [`Self::as_quote_form`] (which
    /// covers the 4-of-12 quote-family carving with `Option<(QuoteForm,
    /// &Sexp)>`). Post-lift the substrate's value-level marker-only
    /// carving-marker matrix closes ONE more cell: the atomic axis via
    /// [`Self::as_atom_kind`] (6-of-12), the residual axis via
    /// [`Self::as_structural_kind`] (2-of-12), the quote-family axis via
    /// `Self::as_quote_form().map(|(qf, _)| qf)` (4-of-12, marker + inner
    /// available via the pre-existing method), and now the unquote-
    /// subset axis via `Self::as_unquote_form` (2-of-12, marker only) —
    /// symmetric with the shape-level marker-only projection family on
    /// [`SexpShape`](crate::error::SexpShape).
    ///
    /// Composition laws (three-way agreement — bindings): for every
    /// `s: &Sexp`,
    /// `s.as_unquote_form() == s.as_unquote().map(|(uf, _)| uf) ==
    ///  s.shape().as_unquote_form() ==
    ///  s.as_quote_form().and_then(|(qf, _)| qf.as_unquote_form())`.
    /// Pre-lift the unquote-subset carving marker at the value level
    /// was reachable only via one of these three-step compositions —
    /// either through the parent [`Self::as_unquote`] projection
    /// (discarding the inner), through the shape algebra
    /// (`shape().as_unquote_form()`), or through the parent quote-family
    /// projection composed with the 2-of-4 subset gate
    /// [`QuoteForm::as_unquote_form`]. Post-lift the projection lands at
    /// ONE typed method on the value algebra, and all three compositions
    /// are pinned as agreement laws (see
    /// `sexp_as_unquote_form_agrees_with_as_unquote_map_marker_for_every_variant`,
    /// `sexp_as_unquote_form_agrees_with_shape_as_unquote_form_for_every_variant`,
    /// and
    /// `sexp_as_unquote_form_agrees_with_as_quote_form_and_quote_form_as_unquote_form_for_every_variant`
    /// in this module). A regression that drifts any of the four
    /// projections from the others surfaces immediately.
    ///
    /// Symmetric with [`Self::as_atom_kind`] and [`Self::as_structural_kind`]
    /// on the marker-only shape (returns just the closed-set marker, no
    /// inner-payload borrow) — where [`Self::as_quote_form`] and
    /// [`Self::as_unquote`] surface both the marker AND the wrapped
    /// inner `&Sexp` (because the four quote-family arms and the two
    /// substitution arms structurally carry a boxed inner value),
    /// `as_unquote_form` returns a marker-only projection: consumers that
    /// need the wrapped inner reach the marker-plus-inner sibling
    /// [`Self::as_unquote`], while consumers that only need the closed-
    /// set carving-marker identity (typed-pattern matchers, diagnostic
    /// filters, coverage sweeps, LSP/REPL structural-navigation gates)
    /// reach this projection and never allocate the tuple.
    ///
    /// Composes cleanly with [`UnquoteForm::marker`] to project the value-
    /// level substitution carving membership onto its canonical marker
    /// string (`,` / `,@`):
    /// `s.as_unquote_form().map(UnquoteForm::marker)` — the marker-string
    /// witness for the substitution subset, sibling to
    /// `s.as_atom_kind().map(AtomKind::label)` on the atomic axis, both
    /// routing through the closed-set marker enum's canonical-vocabulary
    /// projection at ONE canonical site (`UnquoteForm::marker` —
    /// itself composed through `QuoteForm::prefix`).
    ///
    /// Structural identity (pinned as a truth-table by
    /// `sexp_as_unquote_form_projects_each_variant_to_canonical_unquote_form`
    /// and `sexp_as_unquote_form_rejects_non_unquote_subset_outer_shapes`):
    ///   * `as_unquote_form() == Some(UnquoteForm::Unquote)`  iff `matches!(self, Sexp::Unquote(_))`
    ///   * `as_unquote_form() == Some(UnquoteForm::Splice)`   iff `matches!(self, Sexp::UnquoteSplice(_))`
    ///   * `as_unquote_form() == None`                        iff `!matches!(self, Sexp::Unquote(_) | Sexp::UnquoteSplice(_))`
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the substitution-
    /// subset carving marker at the value level becomes a NAMED
    /// primitive on the substrate's `Sexp` algebra rather than a per-
    /// site composition through either [`Self::as_unquote`] (discarding
    /// its `&Sexp` inner) or [`Self::shape`] (walking through the full
    /// 12-variant `SexpShape` closed set to arrive at the 2-of-12
    /// carving marker) or the parent [`Self::as_quote_form`] combined
    /// with [`QuoteForm::as_unquote_form`] (the 2-of-4 subset gate).
    /// THEORY.md §II.1 invariant 2 — free middle; every consumer that
    /// wants the substitution-subset carving identity without needing
    /// the wrapped inner (a future `tatara-check` predicate
    /// `(check-value-projects-to-unquote-subset …)` that filters
    /// diagnostics keyed on the substitution-subset cohort; a future LSP
    /// structural-navigation filter that keys on the substitution-subset
    /// carving membership at the value level; a future
    /// `TypedRewriter<TemplateOp>` sweep that walks `Sexp` values whose
    /// substitution-arm identity is `Some(UnquoteForm::_)` regardless of
    /// inner payload identity; a future REPL pretty-printer that chooses
    /// rendering paths keyed on the value-level substitution carving
    /// marker without needing the inner payload) binds to ONE typed
    /// method on the value algebra. THEORY.md §VI.1 — generation over
    /// composition; the (Sexp variant, UnquoteForm variant) pairing
    /// binds at ONE inherent method on the algebra rather than at three
    /// parallel compositions (`as_unquote().map(…)`, `shape()
    /// .as_unquote_form()`, `as_quote_form().and_then(|(qf, _)|
    /// qf.as_unquote_form())`), so a regression that drifts ONE
    /// composition's pairing from the others cannot reach the substrate's
    /// runtime — the type system binds all three compositions to the
    /// projection's single emission shape.
    ///
    /// Frontier inspiration: MLIR's `mlir::dyn_cast<UnquoteFamilyOp>(op)
    /// .map(|op| op.marker())` — every typed rewriter that only needs
    /// the op-family identity (without the op's operands) binds to the
    /// typed-downcast projection composed with an operand-discarding
    /// marker extract; `Sexp::as_unquote_form` is the marker-only peer
    /// on the substrate's `Sexp` algebra, with `Option<UnquoteForm>`
    /// standing in for MLIR's `Optional<OperationName>` marker-only
    /// downcast result. Racket's `syntax-parse` `~or* (~unquote _)
    /// (~unquote-splice _)` — every syntax-class pattern that keys on
    /// the substitution-subset marker identity without binding the
    /// inner form; `Sexp::as_unquote_form` is the Rust-typed peer that
    /// surfaces the marker identity through a single primitive on the
    /// syntax algebra.
    #[must_use]
    pub fn as_unquote_form(&self) -> Option<UnquoteForm> {
        self.as_unquote().map(|(uf, _)| uf)
    }

    /// Decompose a quote-family form into its typed marker and inner
    /// expression — `Some((QuoteForm::Quote, inner))` iff this is `'x`
    /// (a [`Sexp::Quote`] wrapper), `Some((QuoteForm::Quasiquote, inner))`
    /// iff this is `` `x `` (a [`Sexp::Quasiquote`] wrapper),
    /// `Some((QuoteForm::Unquote, inner))` iff this is `,x` (a
    /// [`Sexp::Unquote`] wrapper), `Some((QuoteForm::UnquoteSplice, inner))`
    /// iff this is `,@x` (a [`Sexp::UnquoteSplice`] wrapper), `None` for
    /// every other shape (Nil, Atom, List).
    ///
    /// This is the *quote-family projection* — the typed-marker peer of
    /// [`Sexp::as_unquote`] generalized across all four homoiconic
    /// prefix-wrappers. Where [`Sexp::as_unquote`] keys the macro-template
    /// SUBSTITUTION surface on the closed pair `{Unquote, Splice}` (the
    /// two prefixes whose template-time semantic is substitution),
    /// `as_quote_form` keys the WIRE-SHAPE surfaces (Display rendering,
    /// Hash discrimination, canonical-form interop) on the closed superset
    /// `{Quote, Quasiquote, Unquote, UnquoteSplice}` — all four prefixes
    /// the reader can tokenize and the writer must round-trip. The
    /// `Sexp::as_unquote` projection now derives structurally from
    /// `as_quote_form`'s output via [`QuoteForm::as_unquote_form`] — the
    /// 2-of-4 subset gate — so the two projections share a SINGLE
    /// implementation site on the `Sexp` algebra and the
    /// (Sexp variant, QuoteForm variant) pairing binds at ONE rust
    /// function regardless of whether the consumer wants the substitution
    /// subset or the wire-shape superset.
    ///
    /// Three consumers in this file route through this primitive:
    ///   * `Hash for Sexp` — the four `Quote`/`Quasiquote`/`Unquote`/
    ///     `UnquoteSplice` arms (pre-lift each carrying its own
    ///     `<discr>.hash(h); inner.hash(h)` body) collapse to ONE arm
    ///     that routes through `as_quote_form` and reads the
    ///     discriminator via [`QuoteForm::hash_discriminator`].
    ///   * `Display for Sexp` — the four `write!(f, "<prefix>{inner}")`
    ///     arms (pre-lift each carrying its own literal prefix string)
    ///     collapse to ONE arm that routes through `as_quote_form` and
    ///     reads the prefix via [`QuoteForm::prefix`].
    ///   * [`Sexp::as_unquote`] — derives `Option<(UnquoteForm, &Sexp)>`
    ///     by composing `as_quote_form` with [`QuoteForm::as_unquote_form`]
    ///     (the 2-of-4 subset projection), so the macro-template
    ///     substitution surface inherits the (Sexp variant, marker)
    ///     pairing through this projection's typed dispatch rather than
    ///     re-deriving its own arm-based match.
    ///
    /// The closed-set guarantee on [`QuoteForm`] (exactly
    /// `Quote ⊎ Quasiquote ⊎ Unquote ⊎ UnquoteSplice`) is threaded through
    /// this projection's return tuple, so consumers that pattern-match on
    /// `form: QuoteForm` get rustc-enforced exhaustiveness — a future
    /// `Sexp` wrapper variant must extend `QuoteForm` AND this match arm
    /// together (or stay outside the quote family and project to `None`),
    /// eliminating the silent multi-site extension-drift this lift was
    /// designed to forbid.
    ///
    /// Soft face, like the rest of the `as_*` family on `Sexp`: it
    /// answers "is this form a quote-family marker, and what does it
    /// wrap?" and yields `None` for everything that isn't (skip / fall
    /// through), with no diagnostic.
    ///
    /// Structural identity binding it to the quote-family variants and
    /// its `as_unquote` subset sibling:
    ///   * `as_quote_form() == Some((QuoteForm::Quote, inner))`         iff `self == Sexp::Quote(inner)`
    ///   * `as_quote_form() == Some((QuoteForm::Quasiquote, inner))`    iff `self == Sexp::Quasiquote(inner)`
    ///   * `as_quote_form() == Some((QuoteForm::Unquote, inner))`       iff `self == Sexp::Unquote(inner)`
    ///   * `as_quote_form() == Some((QuoteForm::UnquoteSplice, inner))` iff `self == Sexp::UnquoteSplice(inner)`
    ///   * `as_quote_form().is_some() == matches!(self, Sexp::Quote(_) | Sexp::Quasiquote(_) | Sexp::Unquote(_) | Sexp::UnquoteSplice(_))`
    ///   * `as_unquote() == as_quote_form().and_then(|(qf, inner)| qf.as_unquote_form().map(|uf| (uf, inner)))`
    ///
    /// The returned `&Sexp` borrows the inner box's body verbatim — no
    /// clone, no allocation — same lifetime as `&self` and same posture
    /// as [`Sexp::as_unquote`]'s tail.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition; the
    /// quote-family (Sexp variant, prefix string, hash discriminator)
    /// triple appeared inline at three sites (`Hash for Sexp`,
    /// `Display for Sexp`, `as_unquote`) — well past the ≥2 PRIME-DIRECTIVE
    /// trigger once the structural shape is named. THEORY.md §V.1 —
    /// knowable platform; the quote-family typed-marker projection becomes
    /// a NAMED primitive on the substrate's `Sexp` algebra rather than
    /// per-site inline matches paired with per-site discriminator literals
    /// and prefix literals. THEORY.md §II.1 invariant 1 — typed entry; the
    /// reader's prefix-to-variant dispatch ([`crate::reader::read_quoted`])
    /// AND the Display impl's variant-to-prefix dispatch are dual
    /// typed-entry / typed-exit gates over the same closed set; the
    /// `QuoteForm` algebra threads BOTH gates through ONE typed enum so a
    /// regression that drifts one side's prefix from the other (e.g. the
    /// reader gains a fifth prefix but the Display impl doesn't) is no
    /// longer a silent two-site divergence — rustc binds both sides to
    /// the same closed-set enum. THEORY.md §II.1 invariant 2 — free
    /// middle; the three consumers (Hash, Display, `as_unquote`) route
    /// through the SAME projection, so a regression that drifts ONE
    /// consumer's (Sexp variant, marker) pairing from the others cannot
    /// reach the substrate's runtime.
    ///
    /// Frontier inspiration: Racket's `syntax-parse` `~or* (~quote stx)
    /// (~quasiquote stx) (~unquote stx) (~unquote-splice stx)` pattern —
    /// every macro-template pattern over `'`/`` ` ``/`,`/`,@` binds to
    /// ONE typed decomposition that surfaces the marker identity
    /// alongside the inner expression; the substrate's `as_quote_form` is
    /// the Rust-typed peer of that pattern, lifted onto the `Sexp`
    /// algebra with `QuoteForm` standing in for Racket's pattern-class
    /// identity at the homoiconic prefix surface. MLIR's typed-IR
    /// projection `mlir::dyn_cast<QuoteFamilyOp>(op)` — the typed downcast
    /// from a polymorphic IR node onto a closed-set op family is the MLIR
    /// idiom; `as_quote_form` is the unstructured-projection peer on the
    /// substrate's `Sexp` algebra, with `Option<(QuoteForm, &Sexp)>`
    /// standing in for MLIR's typed downcast result.
    pub fn as_quote_form(&self) -> Option<(QuoteForm, &Sexp)> {
        match self {
            Self::Quote(inner) => Some((QuoteForm::Quote, inner)),
            Self::Quasiquote(inner) => Some((QuoteForm::Quasiquote, inner)),
            Self::Unquote(inner) => Some((QuoteForm::Unquote, inner)),
            Self::UnquoteSplice(inner) => Some((QuoteForm::UnquoteSplice, inner)),
            _ => None,
        }
    }

    /// Soft projection onto the closed-set [`QuoteForm`] quote-family
    /// carving marker — the 4-of-12 carving of the [`SexpShape`] algebra
    /// covering the four homoiconic prefix-wrappers ([`Self::Quote`],
    /// [`Self::Quasiquote`], [`Self::Unquote`], [`Self::UnquoteSplice`]).
    /// Returns `Some(QuoteForm::Quote)` iff this is `'x` (a
    /// [`Self::Quote`] wrapper), `Some(QuoteForm::Quasiquote)` iff this
    /// is `` `x `` (a [`Self::Quasiquote`] wrapper),
    /// `Some(QuoteForm::Unquote)` iff this is `,x` (a [`Self::Unquote`]
    /// wrapper), `Some(QuoteForm::UnquoteSplice)` iff this is `,@x` (a
    /// [`Self::UnquoteSplice`] wrapper), `None` for every other outer
    /// shape ([`Self::Nil`], every [`Self::Atom`] variant, [`Self::List`]).
    ///
    /// Direct value-level peer of the shape-level projection
    /// [`SexpShape::as_quote_form`](crate::error::SexpShape::as_quote_form)
    /// — the pair `(Sexp::as_quote_form_marker, SexpShape::as_quote_form)`
    /// binds the (Sexp value, QuoteForm carving marker) pairing at ONE
    /// typed method on each algebra, closing the quote-family cell of
    /// the (Sexp value → carving marker) matrix at the marker-only
    /// value-level projection surface. Marker-only sibling of
    /// [`Self::as_quote_form`] (which returns `Option<(QuoteForm, &Sexp)>`
    /// — marker + wrapped inner). Post-lift the substrate's value-level
    /// marker-only carving-marker matrix closes its FINAL cell: the
    /// atomic axis via [`Self::as_atom_kind`] (6-of-12), the residual
    /// axis via [`Self::as_structural_kind`] (2-of-12), the unquote-
    /// subset axis via [`Self::as_unquote_form`] (2-of-12), and NOW the
    /// quote-family axis via `Self::as_quote_form_marker` (4-of-12) —
    /// symmetric with the shape-level marker-only projection family on
    /// [`SexpShape`](crate::error::SexpShape).
    ///
    /// Composition laws (two-way agreement — bindings): for every
    /// `s: &Sexp`,
    /// `s.as_quote_form_marker() == s.as_quote_form().map(|(qf, _)| qf)
    ///  == s.shape().as_quote_form()`. Pre-lift the quote-family carving
    /// marker at the value level was reachable only via one of these
    /// two-step compositions — either through the parent
    /// [`Self::as_quote_form`] projection (discarding the wrapped inner
    /// via `.map(|(qf, _)| qf)`) or through the shape algebra
    /// (`s.shape().as_quote_form()`, walking the full 12-variant
    /// [`SexpShape`](crate::error::SexpShape) closed set to arrive at
    /// the 4-of-12 carving marker). Post-lift the projection lands at
    /// ONE typed method on the value algebra, and both compositions
    /// are pinned as agreement laws (see
    /// `sexp_as_quote_form_marker_agrees_with_as_quote_form_map_marker_for_every_variant`
    /// and
    /// `sexp_as_quote_form_marker_agrees_with_shape_as_quote_form_for_every_variant`
    /// in this module).
    ///
    /// Superset-gate contract with [`Self::as_unquote_form`]: for every
    /// `s: &Sexp`, `s.as_unquote_form().is_some()` implies
    /// `s.as_quote_form_marker().is_some()` (the 2-of-12 substitution
    /// subset is a proper subset of the 4-of-12 quote family). The two
    /// non-substitution quote-family wrappers ([`Self::Quote`] and
    /// [`Self::Quasiquote`]) satisfy `as_quote_form_marker().is_some()`
    /// AND `as_unquote_form().is_none()` — the value-level image of the
    /// 2-of-4 subset gate [`QuoteForm::as_unquote_form`]. Pinned by
    /// `sexp_as_quote_form_marker_extends_as_unquote_form_to_full_quote_family`.
    ///
    /// Structural identity binding it to the quote-family variants:
    ///   * `as_quote_form_marker() == Some(QuoteForm::Quote)`         iff `matches!(self, Sexp::Quote(_))`
    ///   * `as_quote_form_marker() == Some(QuoteForm::Quasiquote)`    iff `matches!(self, Sexp::Quasiquote(_))`
    ///   * `as_quote_form_marker() == Some(QuoteForm::Unquote)`       iff `matches!(self, Sexp::Unquote(_))`
    ///   * `as_quote_form_marker() == Some(QuoteForm::UnquoteSplice)` iff `matches!(self, Sexp::UnquoteSplice(_))`
    ///   * `as_quote_form_marker() == None`                           iff `!matches!(self, Sexp::Quote(_) | Sexp::Quasiquote(_) | Sexp::Unquote(_) | Sexp::UnquoteSplice(_))`
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the quote-
    /// family carving marker at the value level becomes a NAMED
    /// primitive on the substrate's `Sexp` algebra rather than a per-
    /// site two-step composition through either [`Self::as_quote_form`]
    /// (discarding its `&Sexp` inner) or [`Self::shape`] (walking through
    /// the full 12-variant [`SexpShape`](crate::error::SexpShape) closed
    /// set to arrive at the 4-of-12 carving marker). THEORY.md §II.1
    /// invariant 2 — free middle; every consumer that wants the quote-
    /// family carving identity without needing the wrapped inner (a
    /// future `tatara-check` predicate `(check-value-projects-to-quote-
    /// family …)` that filters diagnostics keyed on the quote-family
    /// cohort; a future LSP structural-navigation filter that keys on
    /// the quote-family carving membership at the value level; a
    /// future `TypedRewriter<QuoteFamilyOp>` sweep that walks `Sexp`
    /// values whose quote-family arm identity is `Some(QuoteForm::_)`
    /// regardless of inner payload identity; a future REPL pretty-
    /// printer that chooses rendering paths keyed on the value-level
    /// quote-family carving marker without needing the inner payload)
    /// routes through ONE typed method rather than reaching into one of
    /// the two composition sites, and both compositions are pinned as
    /// agreement laws so a regression that drifts ONE composition's
    /// pairing from the other cannot reach the substrate's runtime.
    /// THEORY.md §VI.1 — generation over composition; the (Sexp variant,
    /// QuoteForm variant) pairing binds at ONE inherent method on the
    /// algebra rather than at two parallel compositions, so a future
    /// extension (e.g. a fifth `Sexp` quote-family wrapper) lands at
    /// ONE match arm in the parent `as_quote_form` projection and
    /// inherits through this method's structural composition.
    ///
    /// Frontier inspiration: MLIR's `mlir::dyn_cast<QuoteFamilyOp>(op)
    /// .map(|op| op.marker())` — every typed rewriter that only needs
    /// the op-family identity (without the op's operands) binds to the
    /// typed-downcast projection composed with an operand-discarding
    /// marker extract; `Sexp::as_quote_form_marker` is the marker-only
    /// peer on the substrate's `Sexp` algebra, with
    /// `Option<QuoteForm>` standing in for MLIR's
    /// `Optional<OperationName>` marker-only downcast result. Racket's
    /// `syntax-parse` `~or* (~quote _) (~quasiquote _) (~unquote _)
    /// (~unquote-splice _)` — every syntax-class pattern that keys on
    /// the quote-family marker identity without binding the inner form;
    /// `Sexp::as_quote_form_marker` is the Rust-typed peer that
    /// surfaces the marker identity through a single primitive on the
    /// syntax algebra.
    #[must_use]
    pub fn as_quote_form_marker(&self) -> Option<QuoteForm> {
        self.as_quote_form().map(|(qf, _)| qf)
    }

    /// Quote-family projection, asserted-total face of [`Sexp::as_quote_form`].
    /// Returns `(QuoteForm, &Sexp)` verbatim — same borrowed-inner posture,
    /// same closed-set marker — but panics with [`QUOTE_FAMILY_PROJECTION_INVARIANT`]
    /// instead of yielding `None` for non-quote-family variants. Use AFTER
    /// an outer pattern match has narrowed the discriminant union to the
    /// quote family (`Sexp::Quote(_) | Sexp::Quasiquote(_) | Sexp::Unquote(_) |
    /// Sexp::UnquoteSplice(_)`); the panic message states the invariant the
    /// caller's outer pattern already proves.
    ///
    /// Pre-lift the five production-site quote-family-arm consumers —
    /// `Hash for Sexp::hash_discriminator`, `Display for Sexp::prefix`,
    /// `domain::sexp_shape`, `domain::sexp_to_json`, `interop::iac_forge_tag` —
    /// each carried a verbatim copy of the 4-arm wildcard pattern AND a
    /// verbatim copy of the inline
    /// `.as_quote_form().expect("matched quote-family variant must project
    /// to Some via as_quote_form")` re-projection. The `(pattern, expect
    /// message)` pair appeared bit-for-bit at FIVE sites. Post-lift the
    /// expect message lives at ONE named const and the projection-with-
    /// assertion lives at ONE primitive on the `Sexp` algebra; the five
    /// callsites collapse to ONE typed query each. A future quote-family
    /// extension that drifts ONE site's panic text from the others becomes
    /// structurally impossible (one const, one method); a future site that
    /// needs the same "outer-narrowed, total projection" shape lands on
    /// this primitive directly without re-deriving the expect literal.
    ///
    /// `#[track_caller]` ensures a panic surfaces the consumer's source
    /// position, not this projection's — so the diagnostic stays
    /// load-bearing under the lift.
    ///
    /// Sibling posture to the `expect_*` family of typed-projection
    /// asserted-total faces across the substrate's closed-set algebras
    /// (`Option::expect`, `Result::expect`) — the assertion is the same
    /// shape, the message is named on the algebra it asserts about.
    ///
    /// # Panics
    ///
    /// Panics with [`QUOTE_FAMILY_PROJECTION_INVARIANT`] if `self` is not
    /// a quote-family variant. The outer pattern match at every caller
    /// site is the proof of the invariant; the panic is the static
    /// fall-through for a regression that drifts that proof.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition; the
    /// (4-arm wildcard pattern, expect re-projection) pair appeared bit-
    /// for-bit at five production sites — well past the ≥2 PRIME-DIRECTIVE
    /// trigger. THEORY.md §V.1 — knowable platform; the panic message and
    /// the projection-with-assertion are now ONE named primitive on the
    /// substrate's `Sexp` algebra, structurally binding the invariant
    /// across every consumer that asserts an outer narrowing.
    #[must_use]
    #[track_caller]
    pub fn expect_quote_form(&self) -> (QuoteForm, &Sexp) {
        self.as_quote_form()
            .expect(QUOTE_FAMILY_PROJECTION_INVARIANT)
    }
}

/// Static panic message for [`Sexp::expect_quote_form`]'s asserted-total
/// face of the quote-family projection. Pre-lift this literal appeared
/// inline at five `.expect(...)` callsites (`Hash for Sexp`,
/// `Display for Sexp`, `domain::sexp_shape`, `domain::sexp_to_json`,
/// `interop::iac_forge_tag`); post-lift it lives at ONE named const so a
/// regression that drifts the diagnostic at one site silently from the
/// others becomes structurally impossible. Sibling to the per-projection
/// asserted-total faces across the substrate's typed algebras — the
/// message names the invariant the outer pattern proves, not the
/// substring grep'able by tests.
pub const QUOTE_FAMILY_PROJECTION_INVARIANT: &str =
    "matched quote-family variant must project to Some via as_quote_form";

/// Closed-set typed identifier for the four homoiconic prefix-wrappers in
/// the substrate's `Sexp` algebra — `'x` ([`Sexp::Quote`]), `` `x ``
/// ([`Sexp::Quasiquote`]), `,x` ([`Sexp::Unquote`]), `,@x`
/// ([`Sexp::UnquoteSplice`]) — paired with the projections each consumer
/// surface needs ([`Self::prefix`] for [`crate::ast::Sexp`]'s `Display`
/// impl AND the reader's prefix dispatch dual, [`Self::hash_discriminator`]
/// for [`crate::ast::Sexp`]'s `Hash` impl, [`Self::as_unquote_form`] for
/// the 2-of-4 subset gate the template-substitution surface keys on).
///
/// Mirror at the homoiconic-prefix-wrapper boundary of the prior-run
/// `UnquoteForm` (template-marker subset, 2 variants),
/// `CompilerSpecIoStage` (disk-persistence surface),
/// `TemplateInvariantKind` (bytecode-runtime surface), `MacroDefHead`
/// (macro-definition-head closed set), and `KwargPath` (kwargs-path-shape
/// surface) closed-set lifts: those enums key their respective rejection
/// or projection variants on a typed identity carried inside the variant's
/// data shape; this enum keys the FOUR distinct quote-family rendering /
/// hashing / template-substitution sites on a typed marker identity.
/// Adding a fifth homoiconic prefix-wrapper (e.g., a hypothetical `,~`
/// reverse-unquote) requires extending this enum, which rustc-enforces
/// matching at every projection site (`prefix`, `hash_discriminator`,
/// `as_unquote_form`, plus `Sexp::as_quote_form`'s match arm) — the closed
/// set becomes a TYPE rather than four `&'static str` / `u8` literals that
/// could drift independently across `Sexp::Display`'s prefix arm and
/// `Sexp::Hash`'s discriminator arm and the reader's prefix dispatch.
///
/// Subset-gate relationship to [`UnquoteForm`]: the template-substitution
/// surface's [`Sexp::as_unquote`] is now `as_quote_form().and_then(|(qf,
/// inner)| qf.as_unquote_form().map(|uf| (uf, inner)))` — the 2-of-4
/// projection lives at ONE site on this algebra ([`Self::as_unquote_form`])
/// rather than being re-derived at every consumer that wants only the
/// `{Unquote, UnquoteSplice}` subset. A future enum variant that joins
/// the template-substitution subset (e.g. a typed `defalias`-projected
/// fifth marker) extends [`UnquoteForm`] AND
/// [`Self::as_unquote_form`]'s arm together, with rustc binding the
/// extension through the projection's `Option` return type.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 — typed entry; the
/// homoiconic-prefix-wrapper dispatch (the reader's prefix-to-variant
/// gate AND the Display impl's variant-to-prefix dual) IS the rust-level
/// typed-entry / typed-exit gate, and naming its closed-set identity
/// lifts the gate from per-site literal-pair discipline to ONE typed
/// enum the substrate's diagnostic promotions hang off of.
/// THEORY.md §V.1 — knowable platform; the closed set of homoiconic
/// prefix-wrappers becomes a TYPE rather than four `&'static str` / `u8`
/// literals scattered across Hash / Display / interop / sexp_shape — a
/// typo in any one site is no longer a runtime drift but a compile error
/// against the typed projection. THEORY.md §VI.1 — generation over
/// composition; the typed enum lands the structural-completeness floor
/// for the quote-family surface, parallel to how `UnquoteForm` lands it
/// for the template-marker subset and `MacroDefHead` for the
/// macro-definition-head surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, tatara_lisp_derive::ClosedSet)]
#[closed_set(via = "prefix", display, generate_unknown = "quote form")]
pub enum QuoteForm {
    /// `'x` — literal-quote prefix. The `'` marker; the inner expression
    /// is NOT subject to macro substitution. Projects to NO
    /// `UnquoteForm` (the template-substitution surface ignores quote).
    Quote,
    /// `` `x `` — quasi-quote prefix. The `` ` `` marker; the inner
    /// expression is the template body inside which `,` and `,@` mark
    /// substitution points. Projects to NO `UnquoteForm` (a quasi-quote
    /// is the substitution SCOPE, not a substitution itself).
    Quasiquote,
    /// `,x` — single-value substitution. The `,` marker; the inner
    /// symbol is substituted with its bound value at template
    /// expansion. Projects to `UnquoteForm::Unquote` for the
    /// template-substitution surface.
    Unquote,
    /// `,@x` — list-splice substitution. The `,@` marker; the inner
    /// symbol must be bound to a list, whose elements are flattened
    /// into the containing list at template expansion. Projects to
    /// `UnquoteForm::Splice` for the template-substitution surface.
    UnquoteSplice,
}

impl QuoteForm {
    /// The closed set of four homoiconic prefix-wrappers — single
    /// source of truth that drives every per-variant projection
    /// ([`Self::prefix`] / [`fmt::Display`], [`Self::hash_discriminator`],
    /// [`Self::as_unquote_form`], [`Self::iac_forge_tag`],
    /// [`Self::sexp_shape`], [`Self::wrap`], and the [`Self::FromStr`]
    /// decode sweep keyed on [`Self::prefix`]).
    ///
    /// Adding a hypothetical fifth homoiconic prefix-wrapper (e.g.
    /// a `,~` reverse-unquote, a `,?` conditional-unquote, or a
    /// `#'` Common-Lisp function-quote literal) lands at one
    /// [`Self::ALL`] entry plus one arm per projection — exhaustively
    /// checked by the compiler (the `[Self; 4]` array literal forces
    /// the arity) AND by the per-variant truth-table tests below.
    ///
    /// Sibling closed-set lift to every other typed-shape enum the
    /// substrate carries: this crate's own
    /// [`crate::error::SexpShape::ALL`] (the twelve reachable outer
    /// shapes — superset of this enum's four via [`Self::sexp_shape`]),
    /// [`AtomKind::ALL`] (the six atomic-payload kinds — peer axis
    /// on the same algebra, also a 6-of-12 carving of `SexpShape`),
    /// [`crate::error::UnquoteForm::ALL`] (the two template-substitution
    /// markers — proper 2-of-4 subset of THIS enum via
    /// [`Self::as_unquote_form`]), and the cross-crate `tatara-process`
    /// family (`ConditionKind::ALL`, `ProcessPhase::ALL`,
    /// `ProcessSignal::ALL`, `ChannelKind::ALL`, `IntentKind::ALL`,
    /// `LifetimeKind::ALL`, `RequestorKind::ALL`, `ReceiptKind::ALL`,
    /// …) every one of which paired its typed projection with `ALL`
    /// before this lift.
    ///
    /// Future consumers that compose against `ALL`: LSP / REPL
    /// completion for the operator-facing rendered homoiconic prefix
    /// (every `'`/`` ` ``/`,`/`,@` substring an authoring tool would
    /// surface in a completion list keys on this set's projection
    /// through [`Self::prefix`]); `tatara-check` coverage assertions
    /// over which quote-family wrappers reach a `Sexp::Display` /
    /// `Hash for Sexp` / `as_unquote_form` consumer arm at all — the
    /// typed sweep replaces a per-callsite vocabulary of four
    /// `&'static str` / `u8` literals; any future audit-trail metric
    /// jointly labeled by [`Self::prefix`] (e.g.
    /// `tatara_lisp_quote_family_total{prefix="'"}`) — the metric
    /// label set IS [`Self::ALL`] mapped through [`Self::prefix`];
    /// any future structural rewriter (typed analogue of MLIR's
    /// `op.walk<QuoteFormOp>()`) that wants to sweep over every
    /// quote-family wrapper in a typed sequence.
    pub const ALL: [Self; 4] = [
        Self::Quote,
        Self::Quasiquote,
        Self::Unquote,
        Self::UnquoteSplice,
    ];

    /// Canonical `&'static str` prefix that paired with the variant
    /// renders the homoiconic form — `"'"` for [`Self::Quote`],
    /// `` "`" `` for [`Self::Quasiquote`], `","` for [`Self::Unquote`],
    /// `",@"` for [`Self::UnquoteSplice`]. Threaded through
    /// [`crate::ast::Sexp`]'s `Display` impl so the per-variant prefix
    /// rendering lives at ONE site on this algebra rather than four
    /// inline literal strings across the Display arms.
    ///
    /// Structural dual of the reader's [`crate::reader::read_quoted`]
    /// dispatch: the reader maps prefix-tokens to `Sexp::{Quote,
    /// Quasiquote, Unquote, UnquoteSplice}` constructors; this method
    /// maps the typed `QuoteForm` marker back to its canonical prefix
    /// string. Adding a fifth prefix extends both sides — the reader's
    /// tokenizer + dispatch AND this method — with rustc enforcing
    /// the pair through the closed-set enum. Round-trip:
    /// `read(format!("{}{inner}", qf.prefix()))` produces the
    /// `Sexp::*` variant matching `qf`, by construction.
    ///
    /// The `&'static str` lifetime is load-bearing: it lets every
    /// consumer (Display arm, future format strings, future interop
    /// canonical-form taggers) project through this method without
    /// an allocation, parallel to how [`UnquoteForm::marker`]
    /// projects its 2-of-4 subset surface.
    #[must_use]
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Quote => "'",
            Self::Quasiquote => "`",
            Self::Unquote => ",",
            Self::UnquoteSplice => ",@",
        }
    }

    /// Stable, per-variant byte discriminator that paired with the
    /// recursive inner hash builds the substrate's `Hash for Sexp`
    /// projection — `3` for [`Self::Quote`], `4` for
    /// [`Self::Quasiquote`], `5` for [`Self::Unquote`], `6` for
    /// [`Self::UnquoteSplice`]. The byte values are load-bearing
    /// because the expansion cache (`Expander::cache`) keys on the
    /// hash of `(macro_name, args)` — changing a discriminator silently
    /// invalidates every cached expansion AND mis-collides with the
    /// reserved bytes the non-quote-family Hash arms use (`0` for
    /// `Nil`, `1` for `Atom`, `2` for `List`). The closed set ensures
    /// the four arms partition `{3, 4, 5, 6}` injectively against the
    /// reserved bytes — a future quote-family extension must extend
    /// this method AND the non-quote-family arms in lockstep, with
    /// rustc binding the consistency through exhaustiveness over the
    /// closed enum.
    ///
    /// `pub(crate)` because the byte-discriminator surface is an
    /// implementation detail of the substrate's `Hash for Sexp` cache-
    /// key contract; exposing it publicly would leak the cache-key
    /// shape through the API without enabling any external consumer
    /// the public projections (`Sexp::as_quote_form`, `Self::prefix`,
    /// `Self::as_unquote_form`) don't already serve.
    #[must_use]
    pub(crate) fn hash_discriminator(self) -> u8 {
        match self {
            Self::Quote => 3,
            Self::Quasiquote => 4,
            Self::Unquote => 5,
            Self::UnquoteSplice => 6,
        }
    }

    /// Project the 4-of-4 quote-family marker into the 2-of-4
    /// template-substitution subset — `Some(UnquoteForm::Unquote)` for
    /// [`Self::Unquote`], `Some(UnquoteForm::Splice)` for
    /// [`Self::UnquoteSplice`], `None` for [`Self::Quote`] /
    /// [`Self::Quasiquote`] (the literal-quote and quasi-quote
    /// prefixes are wrappers, NOT substitution points). ONE projection
    /// on this algebra the [`crate::ast::Sexp::as_unquote`] derivation
    /// routes through — the (Sexp variant, UnquoteForm marker) pairing
    /// now binds at the typed [`crate::ast::Sexp::as_quote_form`]
    /// projection's output composed with this method's output, instead
    /// of being re-derived per-arm inside `Sexp::as_unquote`.
    ///
    /// The closed-set guarantee on [`UnquoteForm`] (exactly
    /// `Unquote ⊎ Splice`) AND on [`Self`] (exactly
    /// `Quote ⊎ Quasiquote ⊎ Unquote ⊎ UnquoteSplice`) ensures that the
    /// 2-of-4 subset is structurally fixed: a future variant joining
    /// the template-substitution surface extends both enums AND this
    /// method's match arm together, with rustc binding the extension
    /// through the projection's `Option` return type.
    #[must_use]
    pub fn as_unquote_form(self) -> Option<UnquoteForm> {
        match self {
            Self::Unquote => Some(UnquoteForm::Unquote),
            Self::UnquoteSplice => Some(UnquoteForm::Splice),
            Self::Quote | Self::Quasiquote => None,
        }
    }

    /// Canonical iac-forge interop tag — the symbol head the canonical
    /// 2-element-list encoding of a quote-family wrapper uses when
    /// projecting `tatara_lisp::Sexp` into `iac_forge::sexpr::SExpr`:
    /// `"quote"` for [`Self::Quote`], `"quasiquote"` for
    /// [`Self::Quasiquote`], `"unquote"` for [`Self::Unquote`],
    /// `"unquote-splicing"` for [`Self::UnquoteSplice`].
    ///
    /// The mapping is Common-Lisp-canonical: a `,@x` form encodes as
    /// `(unquote-splicing x)` rather than `(unquote-splice x)`. That
    /// tag-string choice is INTENTIONALLY DISTINCT from the substrate's
    /// shorter diagnostic label projected by
    /// [`crate::error::SexpShape::label`] (which renders
    /// `[`Self::UnquoteSplice`]` as `"unquote-splice"` — the shorter
    /// idiom appropriate for `expected …, got unquote-splice` error
    /// surfaces). The two projections key the SAME closed set on TWO
    /// distinct boundaries:
    ///
    /// * `iac_forge_tag` — cross-crate canonical form, BLAKE3 attestation
    ///   keys, render-cache shape (load-bearing for byte-identical
    ///   inter-crate compatibility with the iac-forge ecosystem).
    /// * `SexpShape::label` — operator-facing diagnostic label,
    ///   `LispError::TypeMismatch.got` rendering, REPL/LSP
    ///   shape-of-witness surface.
    ///
    /// Pre-lift the four canonical iac-forge tag strings lived inline
    /// across four arms in [`crate::interop`]'s
    /// `From<&Sexp> for iac_forge::sexpr::SExpr` impl, paired with the
    /// matching `Sexp::{Quote, Quasiquote, Unquote, UnquoteSplice}`
    /// patterns. The pairing was load-bearing yet only enforced by
    /// callsite discipline at a FOURTH consumer site (alongside `Hash`,
    /// `Display`, and `Sexp::as_unquote`) the prior closed-set
    /// `QuoteForm` lift did not reach (the `iac-forge` feature gate
    /// kept that site's drift risk silent in the default build). After
    /// this lift the interop arms collapse to ONE arm routing through
    /// [`crate::ast::Sexp::as_quote_form`] + this method, so the
    /// (Sexp variant, canonical tag string) pairing binds at ONE site
    /// on the substrate algebra regardless of which consumer surface
    /// (`Hash`, `Display`, `Sexp::as_unquote`, iac-forge interop)
    /// needs it.
    ///
    /// The `&'static str` lifetime is load-bearing: every iac-forge
    /// consumer projects through this method into the canonical
    /// 2-element-list head without an allocation, parallel to how
    /// [`Self::prefix`], [`UnquoteForm::marker`], and
    /// [`crate::error::SexpShape::label`] project their respective
    /// closed-set surfaces. A future homoiconic prefix-wrapper (e.g.
    /// hypothetical `,~` reverse-unquote) extends [`Self`] AND this
    /// method's match arm together — rustc binds the iac-forge
    /// canonical-form surface to the algebra through exhaustiveness.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the
    /// quote-family canonical-form tag set becomes a TYPE projection
    /// on the substrate algebra rather than four `&'static str`
    /// literals scattered across the `interop` arms (parallel to how
    /// `Self::prefix` lifts the Display↔reader prefix and
    /// `Self::hash_discriminator` lifts the cache-key bytes).
    /// THEORY.md §VI.1 — generation over composition; the (Sexp
    /// variant, iac-forge tag) pairing appeared at the four
    /// `interop.rs` arms — past the ≥2 PRIME-DIRECTIVE trigger once
    /// the structural shape is named. THEORY.md §II.1 invariant 1 —
    /// typed entry; the cross-crate canonical-form projection IS the
    /// typed-exit gate at the iac-forge boundary, and naming its
    /// closed-set tag identity lifts the gate from per-site literal
    /// discipline to ONE method the iac-forge round-trip discipline
    /// binds against.
    #[must_use]
    pub fn iac_forge_tag(self) -> &'static str {
        match self {
            Self::Quote => "quote",
            Self::Quasiquote => "quasiquote",
            Self::Unquote => "unquote",
            Self::UnquoteSplice => "unquote-splicing",
        }
    }

    /// Project the typed marker into its matching [`crate::error::SexpShape`]
    /// variant — `Quote → SexpShape::Quote`, `Quasiquote → SexpShape::Quasiquote`,
    /// `Unquote → SexpShape::Unquote`, `UnquoteSplice → SexpShape::UnquoteSplice`.
    /// ONE projection on the closed-set quote-family algebra the substrate's
    /// outer-shape projection ([`crate::domain::sexp_shape`]) routes through
    /// for the four quote-family arms — so the (Sexp variant, SexpShape
    /// variant) pairing binds at ONE site on the typed algebra rather than
    /// at four byte-identical inline arms in [`crate::domain::sexp_shape`].
    ///
    /// The SIXTH consumer of the closed-set [`QuoteForm`] algebra, sibling
    /// of [`Self::prefix`] (Display / reader prefix-string surface),
    /// [`Self::hash_discriminator`] (Hash cache-key bytes surface),
    /// [`Self::as_unquote_form`] (2-of-4 template-substitution subset gate),
    /// [`Self::iac_forge_tag`] (cross-crate canonical-form tag surface), and
    /// [`Self::wrap`] (reader's marker → `Sexp::*` constructor surface).
    /// Composes with [`SexpShape::label`] to yield the short diagnostic
    /// label string the substrate's `LispError::TypeMismatch.got` slot
    /// renders — the (QuoteForm variant, SexpShape variant, short label)
    /// triple binds end-to-end through the typed algebra so a regression
    /// that drifts the short label silently between the typed marker and
    /// the diagnostic surface is structurally impossible.
    ///
    /// Bidirectional dual: the inverse projection
    /// [`crate::error::SexpShape::as_quote_form`] (12→4, partial)
    /// covers the 4-of-12 carving of [`SexpShape`] this embed reaches.
    /// The pair `(QuoteForm::sexp_shape,
    /// SexpShape::as_quote_form)` forms an `Iso(QuoteForm, QuoteShape ⊂
    /// SexpShape)`: every typed marker round-trips through the embed
    /// (`QuoteForm::sexp_shape(qf).as_quote_form() == Some(qf)` for
    /// every `qf: QuoteForm`), every quote-shape pre-image recovers
    /// the typed marker. The non-quote-family shapes (`Nil`, `List`,
    /// every atomic-payload variant) form the kernel of the inverse —
    /// `as_quote_form` returns `None` for them. See
    /// [`crate::error::SexpShape::as_quote_form`]'s docstring for the
    /// composition law's other direction + disjointness with the
    /// atomic-payload sibling `SexpShape::as_atom_kind`.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the (QuoteForm
    /// variant, SexpShape variant) pairing becomes a TYPE projection on
    /// the substrate algebra rather than four inline arms in
    /// [`crate::domain::sexp_shape`]. A typo or swap at the shape-projection
    /// site is no longer a runtime drift but a compile error against the
    /// typed projection. THEORY.md §II.1 invariant 2 — free middle; SIX
    /// consumers of the [`QuoteForm`] algebra now route through ONE typed
    /// closed-set match family, so a regression that drifts ONE consumer's
    /// pairing from the others cannot reach the substrate's runtime.
    /// THEORY.md §VI.1 — generation over composition; the (Sexp variant,
    /// SexpShape variant) pairing appeared at four arms in `sexp_shape` —
    /// past the ≥2 PRIME-DIRECTIVE trigger once the structural shape is
    /// named.
    #[must_use]
    pub fn sexp_shape(self) -> SexpShape {
        match self {
            Self::Quote => SexpShape::Quote,
            Self::Quasiquote => SexpShape::Quasiquote,
            Self::Unquote => SexpShape::Unquote,
            Self::UnquoteSplice => SexpShape::UnquoteSplice,
        }
    }

    /// Project the typed marker back into its matching `Sexp::*` wrapper
    /// variant applied to `inner` — the structural inverse of
    /// [`crate::ast::Sexp::as_quote_form`]. [`Self::Quote`] yields
    /// [`Sexp::Quote`], [`Self::Quasiquote`] yields [`Sexp::Quasiquote`],
    /// [`Self::Unquote`] yields [`Sexp::Unquote`], [`Self::UnquoteSplice`]
    /// yields [`Sexp::UnquoteSplice`], each boxing `inner` into the
    /// corresponding tuple-variant constructor (`fn(Box<Sexp>) -> Sexp`).
    ///
    /// Round-trip identity with [`crate::ast::Sexp::as_quote_form`] — the
    /// structural law every consumer can pin against:
    ///
    /// ```ignore
    /// // for every (qf, inner): qf.wrap(inner.clone()).as_quote_form() == Some((qf, &inner))
    /// // for every Sexp s matching the quote family:
    /// //     let (qf, inner) = s.as_quote_form().unwrap();
    /// //     qf.wrap(inner.clone()) == s
    /// ```
    ///
    /// Consumer: [`crate::reader::read_quoted`] — the FIFTH consumer site
    /// of the closed-set `QuoteForm` algebra (sibling to `Hash for Sexp`'s
    /// `hash_discriminator` arm, `Display for Sexp`'s `prefix` arm,
    /// `Sexp::as_unquote`'s `as_unquote_form` subset-gate composition, and
    /// the feature-gated `From<&Sexp> for iac_forge::SExpr`'s
    /// `iac_forge_tag` arm). Pre-lift the reader's parse dispatch carried
    /// its own parallel closed set: a local `Token::{Quote, Quasiquote,
    /// Unquote, UnquoteSplice}` enum paired with the matching `Sexp::*`
    /// tuple-variant constructors threaded as `fn(Box<Sexp>) -> Sexp`
    /// arguments to `read_quoted`. The (Token variant, Sexp::* constructor)
    /// pairing was load-bearing yet only enforced by callsite discipline
    /// at the FIFTH consumer site the prior `QuoteForm` lifts did not
    /// reach — a regression that swapped `Sexp::Quote` and
    /// `Sexp::Quasiquote` between the parser arms type-checked but
    /// silently corrupted every program's quote-family parse.
    ///
    /// Post-lift the reader's `Token` collapses to ONE typed variant
    /// `Token::Quoted(QuoteForm)`, the parser's four prefix arms collapse
    /// to ONE arm `Some((Token::Quoted(qf), _)) => read_quoted(it,
    /// eof_pos, qf)`, and `read_quoted` routes through this projection to
    /// produce the matching `Sexp::*` variant. The (QuoteForm variant,
    /// Sexp::* constructor) pairing now binds at ONE site on the typed
    /// algebra — rustc enforces exhaustiveness across [`Self`]'s closed
    /// set, so a regression that drifts the (marker, constructor) pair
    /// becomes a typed compile error rather than a silent program-text
    /// corruption.
    ///
    /// The `Sexp` (owned) return type complements [`Sexp::as_quote_form`]'s
    /// `&Sexp` (borrowed) — `wrap` consumes the inner body to build the
    /// new wrapper, `as_quote_form` borrows the inner body from the
    /// existing wrapper. The asymmetry is intentional: at the reader's
    /// parse-then-wrap boundary the inner is fresh from `parse(...)?` and
    /// has no caller-owned binding; the typed `Box::new(inner)` allocation
    /// lives at ONE site rather than four (one per pre-lift parser arm),
    /// so a future allocation-policy change (e.g. arena-allocated wrappers
    /// for span-aware Sexp) lands as ONE edit.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 1 — typed entry; the
    /// reader's prefix-token → Sexp-wrapper gate IS the rust-level
    /// typed-entry gate at the source-text boundary, and naming the
    /// typed projection from [`QuoteForm`] back to the `Sexp::*` wrapper
    /// lifts the gate from per-arm constructor literals to ONE method
    /// the closed-set algebra owns — parallel to how [`Self::prefix`]
    /// lifts the Display↔reader prefix-string surface. THEORY.md §II.1
    /// invariant 2 — free middle; ALL FIVE consumers (Hash, Display,
    /// as_unquote, iac-forge interop, reader's parse) now route through
    /// the SAME closed-set algebra so a regression that drifts ONE
    /// consumer's pairing from the others cannot reach the substrate's
    /// runtime. THEORY.md §V.1 — knowable platform; the (QuoteForm
    /// variant, Sexp::* constructor) pairing becomes a TYPE projection on
    /// the substrate algebra rather than four `fn(Box<Sexp>) -> Sexp`
    /// function pointers threaded as call arguments. A typo or
    /// swap is no longer a runtime drift but a compile error against the
    /// typed projection. THEORY.md §VI.1 — generation over composition;
    /// the (QuoteForm variant, Sexp::* constructor) pairing appeared at
    /// the four reader arms — past the ≥2 PRIME-DIRECTIVE trigger once
    /// the structural shape is named. The typed projection lands the
    /// structural-completeness floor for the reader's quote-family
    /// surface, completing the FIVE-consumer closure of the
    /// `QuoteForm` algebra.
    #[must_use]
    pub fn wrap(self, inner: Sexp) -> Sexp {
        let boxed = Box::new(inner);
        match self {
            Self::Quote => Sexp::Quote(boxed),
            Self::Quasiquote => Sexp::Quasiquote(boxed),
            Self::Unquote => Sexp::Unquote(boxed),
            Self::UnquoteSplice => Sexp::UnquoteSplice(boxed),
        }
    }
}

// `impl fmt::Display for QuoteForm` is generated by
// `#[derive(tatara_lisp_derive::ClosedSet)]` + `#[closed_set(display)]` on
// the enum declaration above — emits the substrate-wide
// `f.write_str(Self::prefix(*self))` block byte-for-byte.

// `impl std::str::FromStr for QuoteForm` + `impl crate::ClosedSet for
// QuoteForm` + `pub struct UnknownQuoteForm(pub String)` are generated by
// `#[derive(tatara_lisp_derive::ClosedSet)]` on the enum declaration
// above. `label` delegates to the inherent `QuoteForm::prefix` via
// `#[closed_set(via = "prefix")]` so the domain-canonical
// reader-punctuation projection (`"'" / "`" / "," / ",@"`) stays
// load-bearing at the inherent surface while the trait surface unifies
// every closed-set implementor's projection name onto `label`.
// `#[closed_set(generate_unknown = "quote form")]` emits the typed
// parse-rejection carrier with the substrate-wide `Debug + Clone +
// PartialEq + Eq + thiserror::Error` derives and the `#[error("unknown
// quote form: {0}")]` annotation byte-for-byte; the explicit label pins
// the pre-lift wording even though the auto-derived
// `pascal_to_spaced_lowercase("QuoteForm")` projects to the same
// `"quote form"` literal. The FromStr decode is a linear sweep over
// `QuoteForm::ALL` keyed on `prefix`: every successful decode round-trips
// through `prefix()`, cross-axis labels from `SexpShape` (`"quote" /
// "quasiquote" / ...`) and `iac_forge_tag` (`"unquote-splicing"`) reject —
// pinned by `quote_form_prefix_round_trips_through_from_str` +
// `quote_form_from_str_rejects_sexp_shape_labels_on_homoiconic_prefix_axis`.

/// Iterate over the argument tails of every form in `forms` whose call head
/// matches `keyword` — the *slice-side* sibling of [`Sexp::as_call_to`].
/// Where [`Sexp::as_call_to`] answers "is THIS form a call to `K`, and what
/// are its arguments?" on ONE form, `iter_calls_to` answers "which forms
/// in this SLICE are calls to `K`, and what are their arguments?" on a
/// `&[Sexp]`. Yields `&[Sexp]` for each matching form's argument tail
/// (`&form_list[1..]`, the empty slice for a singleton call like `(K)`);
/// non-matching forms — every shape [`Sexp::as_call_to`] rejects — are
/// skipped silently, matching the soft-projection posture the per-form
/// sibling carries.
///
/// Two consumers in [`compile.rs`](crate::compile) route through this
/// primitive:
///   * [`compile_typed::<T>`](crate::compile::compile_typed) — walks every
///     expanded top-level form and compiles every `(T::KEYWORD :k v …)`
///     form into a typed `T`.
///   * [`compile_named_from_forms::<T>`](crate::compile::compile_named_from_forms)
///     — walks every expanded form and compiles every
///     `(T::KEYWORD NAME :k v …)` form into a [`NamedDefinition<T>`](crate::compile::NamedDefinition).
///
/// Before this lift both consumers opened the same `for form in &expanded
/// { if let Some(args) = form.as_call_to(T::KEYWORD) { … } }` walk inline
/// — well past the ≥2 PRIME-DIRECTIVE trigger once the per-form sibling
/// had a name. After this lift the walk lives in ONE function the two
/// dispatchers route through; a regression that drifts ONE consumer's
/// walk from the other (a future emitter that inlines a partial filter,
/// a debug-mode logger that loses track of non-matching forms, a span-
/// aware walk that threads a borrowed `&Sexp` position alongside the
/// tail) becomes structurally impossible because there is exactly ONE
/// implementation both dispatchers consume. A future authoring tool
/// (LSP / REPL / `tatara-check`) that wants to surface "which forms in
/// this program invoke `K`?" binds to ONE function on the slice algebra
/// instead of re-deriving the walk per consumer.
///
/// Closes the soft-dispatch family at the slice level: the per-form
/// projections `{head_symbol, as_call, as_call_to, as_call_to_any}` each
/// answer "what does THIS form's head say?", and the slice-side
/// `iter_calls_to` extends them to "what do THESE forms' heads say,
/// projected through one keyword?". Typed-decoded sibling on the
/// slice algebra: [`iter_calls_to_any`] — the closure-typed extension
/// of THIS function the same way [`Sexp::as_call_to_any`] extends
/// [`Sexp::as_call_to`] on the per-form algebra. The (per-form,
/// slice-side) × (keyword, classifier) 2×2 of soft-dispatch
/// primitives is closed at the slice corner this lift establishes;
/// the closed-form composition binding the slice-side projection to
/// its per-form sibling is the structural identity every consumer
/// can pin against:
///
/// ```ignore
/// iter_calls_to(forms, k) == forms.iter().filter_map(|f| f.as_call_to(k))
/// ```
///
/// Post-lift `iter_calls_to`'s body composes
/// [`iter_calls_to_any`] with a keyword-equality decoder
/// (`|h| (h == keyword).then_some(())`) and drops the decoded unit, so
/// the keyword-typed slice walk IS the typed-decoded slice walk
/// restricted to a constant-keyword classifier. The (slice-side
/// keyword projection, slice-side typed-decoded projection) pair
/// binds at ONE filter-and-fuse implementation on the algebra
/// rather than at two parallel `forms.iter().filter_map(_)` triples
/// that the type system would not catch when one drifts from the
/// other (a future emitter that adds debug logging at one site but
/// not the other, a future span-aware walk that threads borrowed
/// positional metadata through one site but skips the other).
///
/// The yielded `&[Sexp]` slices borrow `&forms[i][1..]` verbatim — no
/// copy, no allocation, same lifetime as [`Sexp::as_call_to`]'s tail.
/// The iterator's lifetime `'a` is the unified outer lifetime of `forms`
/// AND `keyword`: the keyword string must outlive the iterator's borrow
/// of the slice (typical caller passes `T::KEYWORD: &'static str`, which
/// unifies trivially; a caller passing a locally-allocated `&str` ties
/// the iterator to that local). The closure captures `keyword` by move
/// (the `move` keyword on the `filter_map` closure), so each invocation
/// re-derives the head comparison via [`Sexp::as_call_to`]'s `head ==
/// keyword` check at every form — no shared-state, fully Iterator-fused.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// two-site `for + as_call_to` inline walk is past the ≥2 PRIME-DIRECTIVE
/// trigger once the per-form sibling has a name. THEORY.md §V.1 —
/// knowable platform / "make invalid states unrepresentable"; the
/// slice-side projection becomes a NAMED primitive on the substrate's
/// `&[Sexp]` algebra rather than a re-derived for-loop at every consumer
/// site, so authoring tools (REPL, LSP, `tatara-check`) bind to ONE
/// function instead of re-implementing the walk. THEORY.md §II.1
/// invariant 1 — typed entry; the typed-keyword filter on a slice IS the
/// rust-level typed-entry-batch gate (the batch sibling of `as_call_to`'s
/// per-form gate), and naming its single shape lifts the gate from
/// two-site duplication to one rust function the substrate's diagnostic
/// promotions hang off of. THEORY.md §II.1 invariant 2 — free middle;
/// both dispatchers route through the SAME projection, so a regression
/// that drifts one consumer's walk from the other cannot reach the
/// substrate's runtime: the type system binds every consumer to the
/// projection's single emission shape.
///
/// Frontier inspiration: MLIR's `op.getOps<NamedOp>()` — every rewrite
/// pattern over a typed-op block binds to ONE typed-filter iterator
/// regardless of whether it's matching one op kind or batching across a
/// region's contents; the substrate's `iter_calls_to` is the
/// unstructured-projection peer of that iterator, lifted onto the
/// substrate's typed `&[Sexp]` algebra. Racket's `syntax-parse`
/// `~seq (defmacro id args …) …` ellipsis-form — the slice-level
/// matched-keyword filter is the closed-form sibling of `~seq`'s
/// repeated-pattern matcher, translated through pleme-io primitives as
/// ONE `iter_calls_to(forms, keyword)` projection. Tree-sitter's
/// `Query::matches` over a node sequence — the same "iterate the
/// matched forms in a parent" projection, inherited here for the typed
/// `Sexp` algebra without a new IR layer.
pub fn iter_calls_to<'a>(
    forms: &'a [Sexp],
    keyword: &'a str,
) -> impl Iterator<Item = &'a [Sexp]> + 'a {
    iter_calls_to_any(forms, move |h| (h == keyword).then_some(())).map(|(_, args)| args)
}

/// Iterate over the `(decoded, args)` pairs of every form in `forms` whose
/// call head decodes through `decode` — the *slice-side* sibling of
/// [`Sexp::as_call_to_any`]. Where [`Sexp::as_call_to_any`] answers "is
/// THIS form a call whose head decodes through `F`, and what are its
/// arguments?" on ONE form, `iter_calls_to_any` answers "which forms in
/// this SLICE are calls whose heads decode through `F`, and what do they
/// decode to alongside their arguments?" on a `&[Sexp]`. Yields
/// `(decoded, &[Sexp])` for each matching form — the decoded typed
/// witness alongside the matched form's argument tail (`&form_list[1..]`,
/// the empty slice for a singleton call like `(K)`); non-matching forms
/// — every shape [`Sexp::as_call_to_any`] rejects, including calls whose
/// head is present but `decode` returns `None` for — are skipped silently,
/// matching the soft-projection posture the per-form sibling carries.
///
/// Closes the soft-dispatch family at the slice corner this lift
/// establishes — the (per-form, slice-side) × (keyword, classifier) 2×2
/// of soft-dispatch primitives on the `Sexp`/`&[Sexp]` algebras:
///
/// |                | per-form              | slice-side               |
/// |----------------|-----------------------|--------------------------|
/// | keyword        | [`Sexp::as_call_to`]  | [`iter_calls_to`]        |
/// | classifier `F` | [`Sexp::as_call_to_any`] | `iter_calls_to_any` (this) |
///
/// The keyword corner is the constant-classifier projection of the
/// classifier corner: [`iter_calls_to`] now composes through THIS
/// primitive with a `move |h| (h == keyword).then_some(())` decoder
/// and drops the decoded unit, parallel to how
/// `Sexp::as_call_to(k) == Sexp::as_call_to_any(|h| (h ==
/// k).then_some(())).map(|(_, a)| a)` (modulo the discarded `()`) on
/// the per-form algebra. The slice-side filter-and-fuse implementation
/// now lives at ONE site, so a regression that drifts a debug-logging
/// instrumentation, span-aware borrow threading, or fused-iterator
/// invariant from one slice consumer to the other becomes
/// structurally impossible.
///
/// Two plausible future consumer shapes the typed-decoded slice walk
/// admits with no boilerplate:
///   * **Closed-set classifier** — `iter_calls_to_any(forms,
///     MacroDefHead::from_keyword)` walks a slice yielding `(head: MacroDefHead,
///     args: &[Sexp])` for every `(defmacro …)` / `(defpoint-template …)`
///     / `(defcheck …)` form, decoded to the typed `MacroDefHead` enum.
///     Future LSP / `tatara-check` consumers that surface "every
///     defmacro-family form in this buffer with its kind tag" bind to
///     ONE projection rather than a hand-rolled
///     `forms.iter().filter_map(|f| f.as_call_to_any(MacroDefHead::from_keyword))`
///     triple at each consumer site.
///   * **Live-registry classifier** — `iter_calls_to_any(forms, |h|
///     registry.get(h))` walks a slice yielding `(handler: &Handler,
///     args: &[Sexp])` for every form whose head matches a runtime
///     registry. Future REPL / `tatara-check` consumers that route
///     every form through a registry dispatcher bind to ONE
///     projection rather than re-deriving the `filter_map` pattern
///     per consumer surface — sibling shape to
///     [`Expander::expand`](crate::macro_expand::Expander::expand)'s
///     per-form `as_call_to_any(|h| self.macros.get(h))` macro-call
///     dispatch, lifted onto the slice algebra so a batch walk picks
///     up the same dispatch shape without re-derivation.
///
/// The closed-form composition binding the slice-side projection to
/// its per-form sibling is the structural identity every consumer can
/// pin against:
///
/// ```ignore
/// iter_calls_to_any(forms, decode) ==
///     forms.iter().filter_map(|f| f.as_call_to_any(&mut decode))
/// ```
///
/// The yielded `&[Sexp]` slices borrow `&forms[i][1..]` verbatim — no
/// copy, no allocation, same lifetime as [`Sexp::as_call_to_any`]'s
/// tail. `T` is owned because `decode` is `FnMut(&str) -> Option<T>`
/// and a `&'_ str` borrow into the head symbol would not outlive the
/// helper boundary; consumers projecting to a typed `Copy` enum
/// (e.g. `MacroDefHead`) get the value directly per form, consumers
/// projecting to a borrowed `&'static str` (a closed-set head)
/// project to `&'static str` and inherit the static lifetime through
/// the classifier. The closure is `FnMut` (rather than the per-form
/// sibling's `FnOnce`) because the slice walk calls it once per form
/// — a closure that captures mutable state (a counter, a registry
/// cache) maintains that state across the batch walk; a closure with
/// no mutable state is admitted trivially.
///
/// The iterator's lifetime `'a` unifies `forms`'s borrow lifetime
/// with the closure `F`'s captures lifetime: the decoder must outlive
/// the iterator's borrow of the slice, the typical caller passes a
/// `'static` decoder (a `fn` item like `MacroDefHead::from_keyword`,
/// or a closure capturing nothing) which unifies trivially. The
/// closure captures `decode` by move (the `move` keyword on the
/// `filter_map` closure), so each invocation re-borrows it as
/// `&mut decode` and calls [`Sexp::as_call_to_any`] with a fresh
/// `FnOnce`-coerced borrow — no shared-state hazard, fully
/// Iterator-fused.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// per-form classifier sibling [`Sexp::as_call_to_any`] has two
/// production consumers (`macro_def_from` via closed-set classifier
/// `MacroDefHead::from_keyword`, `Expander::expand` via live-registry
/// classifier `|h| self.macros.get(h)`) — past the ≥2 PRIME-DIRECTIVE
/// trigger once the slice-side projection is named. Future
/// authoring-tool surfaces (LSP buffer walks, `tatara-check` batch
/// dispatchers, REPL exhaustive listers) join the family without
/// re-deriving the `filter_map(|f| f.as_call_to_any(_))` triple per
/// consumer. THEORY.md §V.1 — knowable platform; the slice-side
/// typed-decoded projection becomes a NAMED primitive on the
/// substrate's `&[Sexp]` algebra, closing the 2×2 of soft-dispatch
/// primitives the per-form algebra already establishes. THEORY.md
/// §II.1 invariant 2 — free middle; the slice-side keyword filter
/// ([`iter_calls_to`]) now routes through the slice-side classifier
/// filter (THIS function) via the constant-classifier composition, so
/// a regression that drifts the keyword filter's instrumentation
/// from the classifier filter's instrumentation becomes structurally
/// impossible.
///
/// Frontier inspiration: MLIR's
/// `op.walk<OpInterface, OpInterface2, …>([&](auto op) { … })` — the
/// typed-IR walk over a region yielding ops decoded to their typed
/// interface witness IS the slice-side typed-decoded projection on
/// MLIR's op algebra; `iter_calls_to_any` is the unstructured-Rust
/// peer on the substrate's typed `&[Sexp]` algebra, with `decode:
/// FnMut(&str) -> Option<T>` standing in for MLIR's typed-interface
/// dyn-cast bag. Racket's `syntax-parse` `~or* (~datum defmacro)
/// (~datum defpoint-template) (~datum defcheck) (head args …)` over
/// an ellipsis-form — the slice-level matched-set filter decoded to
/// a typed witness is the closed-form sibling of `~or*`'s
/// typed-choice repeater, translated through pleme-io primitives as
/// ONE `iter_calls_to_any(forms, F)` projection.
pub fn iter_calls_to_any<'a, F, T>(
    forms: &'a [Sexp],
    mut decode: F,
) -> impl Iterator<Item = (T, &'a [Sexp])> + 'a
where
    F: FnMut(&str) -> Option<T> + 'a,
    T: 'a,
{
    forms
        .iter()
        .filter_map(move |f| f.as_call_to_any(&mut decode))
}

/// Iterate over the `Result<(decoded, NAME, spec_args)>` triples of every
/// form in `forms` whose call head decodes through `decode` AND carries a
/// positional NAME slot — the *slice-side* sibling of
/// [`Sexp::as_call_to_any`] specialized to the named NAME-then-kwargs
/// form shape, with the named-form structural gate
/// [`crate::compile::split_name_slot`] composed in. Where
/// [`iter_calls_to_any`] answers "which forms in this SLICE are calls
/// whose heads decode through `F`, and what do they decode to alongside
/// their args tail?" on a `&[Sexp]`, `iter_named_calls_to_any` answers
/// the same question AND extracts the borrowed NAME slot AND the
/// remaining spec args tail in ONE projection per matched form, lifting
/// the named-form gate from inside the projection at every consumer
/// site to the slice algebra itself.
///
/// The yielded `Result<(T, &'a str, &'a [Sexp])>` shape carries the
/// classifier's typed witness `T` alongside the BORROWED NAME slot AND
/// the BORROWED spec args tail. Non-matching forms (every shape
/// [`Sexp::as_call_to_any`] rejects, AND every call whose head is
/// present but `decode` returns `None` for) are skipped silently — the
/// classifier filter precedes the named gate, mirroring how
/// [`crate::compile::split_name_slot`] is composed into the projection
/// AFTER the classifier-decoded args tail is already in hand. Matched
/// forms whose NAME slot is missing yield `Err(NamedFormMissingName {
/// keyword })` carrying the classifier-supplied keyword; matched forms
/// whose NAME slot is a non-symbol-or-string yield `Err(NamedFormNonSymbolName
/// { keyword, got })` carrying the same keyword and the typed
/// [`SexpShape`](crate::error::SexpShape) projection of the offending
/// slot. Consumers `.collect::<Result<Vec<_>, _>>()` to short-circuit
/// at the first malformed NAME slot, exactly as
/// [`Expander::expand_and_collect_named_calls_to_any`](crate::macro_expand::Expander::expand_and_collect_named_calls_to_any)
/// short-circuits today via the same `split_name_slot` gate composed
/// inside its projection closure.
///
/// Decoder signature `FnMut(&str) -> Option<(T, &'static str)>` pairs
/// the typed witness `T` with the canonical static keyword threaded
/// through the `NamedFormMissingName.keyword` /
/// `NamedFormNonSymbolName.keyword` slots of the named-form gate — the
/// `&'static` constraint pins the same compile-time discipline
/// [`crate::compile::split_name_slot`]'s `keyword: &'static str`
/// parameter pins at its boundary. A classifier consumer that wants
/// "filter forms by a constant keyword" supplies a constant-classifier
/// decoder `|h| (h == keyword).then_some(((), keyword))`; the
/// [`iter_named_calls_to`] sibling below is exactly that specialization.
///
/// Closes the (per-form, slice-side) × (keyword, classifier) × (bare,
/// named) 2×2×2 cube of soft-dispatch primitives on the substrate's
/// `Sexp`/`&[Sexp]` algebras at the slice-side × classifier × named
/// corner — the cube the per-form algebra
/// (`as_call_to{,_any}`), the slice algebra
/// (`iter_calls_to{,_any}`), and the Expander surface
/// (`expand_and_collect_calls_to{,_any}` /
/// `expand_and_collect_named_calls_to{,_any}`) collectively shape:
///
/// |                | bare-kwargs              | named NAME-then-kwargs                           |
/// |----------------|--------------------------|--------------------------------------------------|
/// | per-form       | [`Sexp::as_call_to_any`] | [`Sexp::as_named_call_to_any`]                   |
/// | slice          | [`iter_calls_to_any`]    | `iter_named_calls_to_any` (this)                 |
/// | expander       | `expand_and_collect_calls_to_any` | `expand_and_collect_named_calls_to_any`  |
///
/// Pre-lift the bare expander surface (`expand_and_collect_calls_to_any`)
/// routed through the slice primitive ([`iter_calls_to_any`]) via a
/// uniform `expand_program + iter_calls_to_any + map + collect`
/// pipeline; the named expander surface
/// (`expand_and_collect_named_calls_to_any`) routed through the
/// BARE expander surface and welded
/// [`crate::compile::split_name_slot`] INSIDE the projection closure —
/// the named gate composition lived at the expander level rather than
/// at the slice level the bare row sat at. Post-lift the named expander
/// surface routes through THIS slice primitive via the SAME
/// `expand_program + iter_named_calls_to_any + map + collect`
/// pipeline shape, so both rows now share the same composition skeleton
/// on the slice algebra — a regression that drifts a future debug-mode
/// logger, span-aware borrow walker, or fused-iterator invariant from
/// one row to the other becomes structurally impossible at the slice
/// boundary.
///
/// Two plausible future consumer shapes the slice-side named-classifier
/// walk admits with no boilerplate:
///   * **Closed-set classifier** — `iter_named_calls_to_any(forms, |h|
///     match h { "defmonitor" => Some((Kind::Monitor, "defmonitor")),
///     "defalertpolicy" => Some((Kind::Alert, "defalertpolicy")), _ =>
///     None }).collect::<Result<Vec<_>, _>>()?` walks a slice of
///     already-expanded forms, yielding the `(typed Kind, NAME, spec
///     args)` triple for every `(defmonitor NAME …)` / `(defalertpolicy
///     NAME …)` form. Future `tatara-check` consumers that already hold
///     expanded forms (the workspace coherence checker walks
///     `checks.lisp`'s post-expansion top-level) bind to ONE projection
///     on the slice algebra rather than re-deriving the
///     `iter_calls_to_any(forms, decode).map(|(decoded, args)| {
///     split_name_slot(args, kw).map(|(name, rest)| (decoded, name,
///     rest)) })` four-step inline composition.
///   * **Live-registry classifier** — `iter_named_calls_to_any(forms,
///     |h| registry.lookup(h).map(|h| (h, h.canonical_label())))` walks
///     a slice of expanded forms, yielding the `(handler reference, NAME,
///     spec args)` triple for every form whose head matches a runtime
///     registry. Future REPL / authoring-tool surfaces that dispatch
///     named forms through a live registry bind to ONE projection,
///     sibling shape to how the macro expander already routes through
///     a live-registry classifier via
///     [`Sexp::as_call_to_any`].
///
/// The closed-form composition binding this slice primitive to its
/// per-form sibling AND to the bare-kwargs slice primitive is the
/// structural identity every consumer can pin against:
///
/// ```ignore
/// iter_named_calls_to_any(forms, decode) ==
///     iter_calls_to_any(forms, decode).map(|(decoded, args)| {
///         let kw = /* keyword the decoder returned alongside decoded */;
///         split_name_slot(args, kw).map(|(name, rest)| (decoded, name, rest))
///     })
/// ```
///
/// The yielded `&'a str` NAME slot and `&'a [Sexp]` spec args tail
/// borrow from `&forms[i]` verbatim — no copy, no allocation, same
/// lifetime as [`Sexp::as_call_to_any`]'s tail. Consumers that need
/// owned ownership of the NAME (`NamedDefinition.name: String`,
/// JSON-serialized payloads) `.to_string()` themselves — pushing the
/// clone to the consumer boundary keeps the primitive allocation-free.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// named-form gate composition lived at the Expander level pre-lift
/// (inside `expand_and_collect_named_calls_to_any`'s projection
/// closure); the slice algebra had no named sibling to the bare
/// [`iter_calls_to_any`]. Post-lift the slice algebra closes at the
/// named corner, and the Expander surface routes through it via the
/// SAME `expand_program + iter + map + collect` pipeline the bare
/// expander surface uses. THEORY.md §V.1 — knowable platform; the
/// slice-side named-classifier walk becomes a NAMED primitive on the
/// substrate's `&[Sexp]` algebra, discoverable by any future authoring
/// tool (LSP, REPL, `tatara-check`) that already holds expanded forms.
/// THEORY.md §II.1 invariant 2 — free middle; the bare and named slice
/// projections share the same `forms.iter().filter_map(_)` skeleton, so
/// a regression that drifts ONE row's instrumentation from the other
/// becomes structurally impossible.
///
/// Frontier inspiration: MLIR's
/// `region.walk<NamedOp>([&](auto op) { auto name = op.getName(); … })`
/// — the typed-IR walk over a region yielding ops decoded to their
/// typed kind with the NAMED-symbol accessor pre-extracted is the MLIR
/// idiom for a named-op visitor; `iter_named_calls_to_any` is the
/// unstructured-Rust peer on the substrate's `&[Sexp]` algebra, with
/// `decode: FnMut(&str) -> Option<(T, &'static str)>` standing in for
/// MLIR's typed-interface dyn-cast bag AND `split_name_slot` standing
/// in for the named accessor. Racket's `syntax-parse` `~or* ((~datum
/// defX) name:id arg ...) ((~datum defY) name:id arg ...)` over an
/// ellipsis-form — the slice-level matched-set named-form filter
/// decoded to a typed witness is the closed-form sibling of `~or*`'s
/// typed-choice repeater with the `name:id` capture binder, translated
/// through pleme-io primitives as ONE projection on the `&[Sexp]`
/// algebra.
pub fn iter_named_calls_to_any<'a, F, T>(
    forms: &'a [Sexp],
    mut decode: F,
) -> impl Iterator<Item = crate::error::Result<(T, &'a str, &'a [Sexp])>> + 'a
where
    F: FnMut(&str) -> Option<(T, &'static str)> + 'a,
    T: 'a,
{
    forms
        .iter()
        .filter_map(move |f| f.as_named_call_to_any(&mut decode))
}

/// Iterate over the `Result<(NAME, spec_args)>` pairs of every form in
/// `forms` whose call head matches `keyword` AND carries a positional
/// NAME slot — the *slice-side* sibling of [`Sexp::as_call_to`]
/// specialized to the named NAME-then-kwargs form shape, with the
/// named-form structural gate [`crate::compile::split_name_slot`]
/// composed in. Where [`iter_calls_to`] answers "which forms in this
/// SLICE are calls to `K`, and what are their args tails?" on a
/// `&[Sexp]`, `iter_named_calls_to` answers the same question AND
/// extracts the borrowed NAME slot AND the remaining spec args tail in
/// ONE projection per matched form.
///
/// Routes through the typed-decoded sibling [`iter_named_calls_to_any`]
/// with a constant-classifier decoder — the same constant-classifier
/// composition [`iter_calls_to`] uses to route through
/// [`iter_calls_to_any`] on the bare-kwargs axis, and that
/// [`crate::macro_expand::Expander::expand_and_collect_named_calls_to`]
/// uses to route through
/// [`crate::macro_expand::Expander::expand_and_collect_named_calls_to_any`]
/// on the Expander surface. The discarded `()` typed witness
/// (`then_some(((), keyword))`) is consumed by the wrapper projection so
/// the consumer's per-form mapper sees only the `(name, spec_args)`
/// borrowed pair, matching the bare projection signature on the named
/// axis.
///
/// `keyword: &'static str` threads verbatim through the
/// `NamedFormMissingName.keyword` / `NamedFormNonSymbolName.keyword`
/// slots of the named-form gate — same `&'static` discipline
/// [`crate::compile::split_name_slot`] pins at its boundary. Consumers
/// that want a runtime keyword whose lifetime is `&'static` (typical:
/// `T::KEYWORD` of a typed-domain witness, a hardcoded literal like
/// `"defcheck"`) bind to this primitive; consumers that want a runtime
/// keyword whose lifetime is shorter use [`iter_named_calls_to_any`]
/// directly with a constant-classifier decoder that converts
/// post-resolution.
///
/// Closes the (slice-side × constant-keyword × named) corner of the
/// soft-dispatch cube — see [`iter_named_calls_to_any`]'s docstring for
/// the cube shape. The closed-form composition binding this primitive
/// to the typed-decoded sibling is the structural identity every
/// consumer can pin against:
///
/// ```ignore
/// iter_named_calls_to(forms, k) ==
///     iter_named_calls_to_any(forms, |h| (h == k).then_some(((), k)))
///         .map(|maybe_triple| maybe_triple.map(|(_, name, args)| (name, args)))
/// ```
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// constant-keyword named slice projection is a CONSEQUENCE of the
/// typed-decoded named slice projection + a constant-classifier
/// decoder, parallel to how [`iter_calls_to`] is a consequence of
/// [`iter_calls_to_any`] on the bare-kwargs axis. THEORY.md §II.1
/// invariant 2 — free middle; both rows of the slice algebra
/// (bare-kwargs, named) route through their classifier sibling via
/// constant-classifier composition, so a regression that drifts ONE
/// row's pipeline from the other becomes structurally impossible.
pub fn iter_named_calls_to<'a>(
    forms: &'a [Sexp],
    keyword: &'static str,
) -> impl Iterator<Item = crate::error::Result<(&'a str, &'a [Sexp])>> + 'a {
    iter_named_calls_to_any(forms, move |h| (h == keyword).then_some(((), keyword)))
        .map(|maybe_triple| maybe_triple.map(|(_, name, args)| (name, args)))
}

/// Render an `Atom::Float`'s `f64` value to a form that re-reads as
/// `Atom::Float` — preserves the float-vs-int typed identity across the
/// `Sexp::Display` → [`crate::reader::read`] round-trip.
///
/// Rust's stdlib `Display` impl for `f64` elides the trailing `.0` for
/// finite integral values: `format!("{}", 1.0_f64) == "1"`,
/// `format!("{}", 100.0_f64) == "100"`. The substrate's reader
/// (via the typed-entry classifier [`Atom::from_lexeme`]) tries
/// `i64::parse` BEFORE `f64::parse`, so a bare `1` re-reads as
/// `Atom::Int(1)` — NOT as `Atom::Float(1.0)`. The default Display rendering therefore drifts the
/// typed identity at the Display→read boundary: `Float(1.0)` round-trips
/// to `Int(1)` and a regression silently coerces an authoring-surface
/// `1.0` slot into the typed `Int` track.
///
/// This helper emits `1.0` for `1.0_f64` and `1.5` for `1.5_f64` — the
/// `.0` suffix is appended IFF the value is finite AND already integral
/// (`n == n.trunc()`). Non-integral values render through the default
/// `f64` Display impl, which already preserves the fractional component
/// (`1.5`, `0.99`, etc.) round-trippably. Non-finite values (`NaN`,
/// `inf`, `-inf`) also fall through to the default impl — they cannot be
/// reliably round-tripped through the reader regardless (the Hash impl
/// already warns about NaN's PartialEq irregularity at the cache-key
/// boundary), so the helper does not paper over that prior limitation.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 — typed entry; the
/// substrate's typed-entry gate distinguishes `Atom::Int` from
/// `Atom::Float`, and the Display→read round-trip is the typed-exit-side
/// mirror that must preserve the distinction. Pre-lift the
/// `Float(integral) → Int(integral)` collapse silently violated the
/// invariant at the round-trip boundary; post-lift the typed identity is
/// preserved. THEORY.md §V.1 — knowable platform; diagnostics that
/// project a `Float(1.0)` slot through `SexpWitness::display` (sourced
/// from `Sexp::to_string()`) used to surface as `got 1` — confusingly
/// identical to the typed `Int(1)` projection. Post-lift the diagnostic
/// shape names the offender's typed identity (`got 1.0`) so operators
/// distinguish "you wrote 1.0 in an int slot" from "you wrote 1 in a
/// kwarg slot the kwarg gate rejected" without re-reading source.
fn fmt_float(n: f64, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if n.is_finite() && n == n.trunc() {
        write!(f, "{n}.0")
    } else {
        write!(f, "{n}")
    }
}

/// Canonical reader-round-trippable rendering of a single atomic payload —
/// `Symbol(s) → "{s}"`, `Keyword(s) → ":{s}"`, `Str(s) → "{s:?}"` (the
/// debug-quoted form: `\"…\"` with embedded `"` and `\` escaped), `Int(n)
/// → "{n}"`, `Float(n)` through [`fmt_float`] so integral values render
/// with the `.0` suffix that preserves the typed-`Float`-vs-typed-`Int`
/// distinction at the Display→read boundary, `Bool(true) → "#t"`,
/// `Bool(false) → "#f"` (the Scheme bool spellings the reader's
/// typed-entry classifier [`Atom::from_lexeme`] dispatches on — `true`
/// / `false` re-read as symbols, NOT as bools — see the CLAUDE.md
/// "Lisp bools" warning).
///
/// This is the *atomic-payload Display surface* — the typed-exit-side
/// peer of [`Atom::from_lexeme`]'s atomic-payload typed-entry surface
/// (the FOURTH and LAST of the per-`Atom`-variant projection sites
/// lifted onto the closed-set algebra, after the typed-exit Display
/// [this impl], JSON [`Atom::to_json`], and iac-forge canonical
/// attestation [`Atom::to_iac_forge_sexpr`] projections — completing
/// the bidirectional typed-entry/typed-exit sweep). Before this lift
/// the per-variant rendering arms
/// lived inline at the `Sexp::Atom(a) => match a { … }` arm of
/// [`fmt::Display for Sexp`]; routing the outer arm through this impl
/// lifts the seven inline sub-arms (the Bool variant splits into
/// `true`/`false` to short-circuit the `if-else` branch) into ONE
/// typed-algebra method the `Sexp` Display arm calls into via
/// `fmt::Display::fmt(a, f)`. Sibling closed-set lift to
/// [`QuoteForm::prefix`] (the four homoiconic prefix wrappers) and
/// [`AtomKind::label`] (the six diagnostic labels) — those name the
/// quote-family and atomic-discriminator pairings at the `Sexp` and
/// `Atom` algebras respectively; this names the atomic-payload
/// rendering at the `Atom` algebra so future consumers of "render a
/// bare atom" land on this impl directly without unwrapping through
/// `Sexp::Atom(_).to_string()` and stripping the outer wrap.
///
/// Three production-site sibling shapes the substrate carries that
/// route through a per-`Atom`-variant projection, all 6/7-arm inline
/// matches pre-lift:
///   * [`fmt::Display for Sexp`]'s atom arm — 7 sub-arms (Bool splits),
///     produces a `fmt::Formatter` body. Post-lift collapses to
///     ONE `fmt::Display::fmt(a, f)` delegation.
///   * [`crate::domain::sexp_to_json`]'s atom arms — 6 inline arms
///     producing `serde_json::Value`. Now lifted onto [`Atom::to_json`]
///     in the sibling pattern this impl's docstring named; the
///     `sexp_to_json` site collapses to ONE `Sexp::Atom(a) =>
///     a.to_json()` arm.
///   * [`crate::interop::iac_forge_impl::From<&Sexp> for SExpr`]'s
///     atom arm (feature-gated `iac-forge`) — 6 inline arms producing
///     `iac_forge::sexpr::SExpr`. Now lifted onto
///     [`Atom::to_iac_forge_sexpr`] in the sibling pattern this impl's
///     docstring named; the interop site collapses to ONE
///     `Sexp::Atom(a) => a.to_iac_forge_sexpr()` arm. THIRD and LAST
///     of the three production-site atom-arm shapes lifted onto the
///     typed `Atom` algebra; the sweep across the Lisp / JSON /
///     iac-forge canonical-form surfaces is complete.
///
/// The (Atom variant, rendered prefix/suffix/body) quadruple now lives
/// at ONE typed-algebra Display impl rather than at seven inline
/// sub-arms inside `Display for Sexp`'s outer Atom arm. A regression
/// that drifts the Bool spelling (`#t`/`#f` vs `true`/`false`) — the
/// CLAUDE.md-pinned reader-round-trip invariant — now lands at ONE
/// site, and the test surface pins each variant's canonical rendering
/// AND the round-trip identity through the reader at the Atom level
/// directly (no Sexp wrap required to exercise the round-trip).
///
/// Bidirectional contract anchored by tests in this module:
///   * `atom_display_renders_each_variant_to_canonical_form` —
///     sweeps `AtomKind::ALL` and pins each variant's canonical
///     rendering byte-for-byte against the pre-lift inline literal,
///     so a future regression that drifts ONE arm (e.g. swaps
///     `#t`/`#f` for `true`/`false`, or strips `Str`'s quote marks)
///     fails loudly.
///   * `sexp_atom_display_arm_routes_through_atom_display_for_every_variant`
///     — pins the lifted boundary: `Sexp::Atom(a).to_string() ==
///     a.to_string()` for every atomic payload variant, AND that
///     both equal the legacy inline rendering. Catches a future
///     drift where one surface's per-variant body changes without
///     the other.
///   * `atom_display_round_trips_through_reader_preserving_typed_identity`
///     — sweeps a representative atom of each variant, renders it
///     via `Atom::Display`, parses the rendering through
///     [`crate::reader::read`], and pins the parsed atom equals
///     the seed atom (modulo `Str`'s debug-quoted spelling — pinned
///     separately because the reader expects unquoted source-level
///     `"foo"`). Pins that the (`Atom::Display`, reader) pair forms
///     a typed round-trip at the atom layer, the same invariant
///     [`fmt_float`]'s `.0` suffix preserves for the float-vs-int
///     distinction at the Sexp layer.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// (Atom variant, canonical rendering) pair appeared inline at THREE
/// production sites (`Display for Sexp`'s 7-sub-arm atom arm,
/// `sexp_to_json`'s 6 atom arms, `From<&Sexp> for SExpr`'s 6 atom arms)
/// — well past the ≥2 PRIME-DIRECTIVE trigger once the structural
/// shape is named. THIS lift retires the Display-surface site by
/// naming the typed primitive on the `Atom` algebra; future runs route
/// the JSON and iac-forge sites through parallel sibling projections
/// (`Atom::to_json`, `Atom::to_iac_forge_sexpr`) the same pattern
/// names. THEORY.md §II.1 invariant 1 — typed entry; the substrate's
/// [`Atom::from_lexeme`] is the typed-entry gate at the atomic-payload
/// boundary (lifted onto the typed [`Atom`] algebra from the reader's
/// pre-lift free function), and this impl is the typed-exit-side
/// mirror — the closed-set [`AtomKind`] algebra now threads BOTH gates
/// through ONE projection family, so a regression that drifts one side's
/// per-variant rendering from the other (e.g. extends `Atom` with a
/// `Char` variant the reader accepts but the writer can't emit) is no
/// longer a silent two-site divergence — rustc binds both sides to
/// the same closed-set enum. THEORY.md §II.1 invariant 2 — free middle;
/// the typed-exit rendering, the reader, the diagnostic surface
/// (`LispError::TypeMismatch.got` slot rendering an atomic witness),
/// and any future authoring tool (LSP / REPL pretty-printer) all
/// route through ONE per-variant rendering rather than per-callsite
/// re-derivation.
///
/// Frontier inspiration: Racket's `(syntax->datum stx)` / `write` pair
/// — where `syntax->datum` unwraps the homoiconic surface to its
/// atomic-payload layer and `write` emits the canonical S-expression
/// rendering bound to the reader's `read` inverse; `Atom::Display`
/// is the substrate's typed-algebra peer at the atomic-payload boundary,
/// with the closed-set [`AtomKind`] standing in for Racket's
/// datum-prim taxonomy. MLIR's `mlir::AsmPrinter::printAttribute` — the
/// typed-IR attribute printer dispatches on the closed-set
/// `AttributeKind` so every printer body for a kind lives at ONE
/// implementation site; `Atom::Display` is the unstructured Rust peer
/// for the `Sexp`/`Atom` algebra, with `fmt::Display` standing in for
/// MLIR's `AsmPrinter` interface.
impl fmt::Display for Atom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Symbol(s) => f.write_str(s),
            Self::Keyword(s) => write!(f, ":{s}"),
            Self::Str(s) => write!(f, "{s:?}"),
            Self::Int(n) => write!(f, "{n}"),
            Self::Float(n) => fmt_float(*n, f),
            Self::Bool(true) => f.write_str("#t"),
            Self::Bool(false) => f.write_str("#f"),
        }
    }
}

impl fmt::Display for Sexp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => f.write_str("()"),
            // The atomic-payload rendering lives at the typed
            // [`fmt::Display for Atom`] impl above — the seven inline
            // sub-arms `Symbol → s`, `Keyword → ":{s}"`, `Str → "{s:?}"`,
            // `Int → "{n}"`, `Float → fmt_float`, `Bool(true) → "#t"`,
            // `Bool(false) → "#f"` all bind at ONE site on the closed-set
            // `Atom` algebra rather than at this outer arm. A future
            // atomic-kind extension (e.g. `Char` for `#\x` reader syntax,
            // `Bigint` for arbitrary-precision integers) extends `Atom`'s
            // Display impl once and this arm picks up the new variant
            // for free.
            Self::Atom(a) => fmt::Display::fmt(a, f),
            Self::List(xs) => {
                f.write_str("(")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ")?;
                    }
                    write!(f, "{x}")?;
                }
                f.write_str(")")
            }
            // The four quote-family variants share the
            // `write!(f, "<prefix>{inner}")` Display shape — all route
            // through `as_quote_form`'s typed-marker projection so the
            // per-variant prefix (`'`, `` ` ``, `,`, `,@`) binds at ONE
            // site on the closed-set `QuoteForm` algebra and the
            // recursive `inner` rendering composes through the unified
            // Display arm. The (prefix, variant) pairing IS the structural
            // dual of the reader's `read_quoted` (prefix, variant-ctor)
            // dispatch — naming it once threads the round-trip discipline
            // through ONE rust function the reader and the Display impl
            // both bind against.
            Self::Quote(_) | Self::Quasiquote(_) | Self::Unquote(_) | Self::UnquoteSplice(_) => {
                let (qf, inner) = self.expect_quote_form();
                write!(f, "{}{inner}", qf.prefix())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── head_symbol: the operator-position projection ───────────────────
    //
    // `head_symbol` lifts the `self.as_list()?.first()?.as_symbol()` chain
    // that recurred at four soft-dispatch sites (compile.rs `compile_typed`
    // + `compile_named_from_forms`, macro_expand.rs `Expander::expand` +
    // `macro_def_from`) into ONE named query on the Sexp algebra. These
    // tests pin its contract directly; the existing dispatch tests in
    // compile.rs / macro_expand.rs are the path-uniformity guards proving
    // the four sites route through it without behavior drift.

    #[test]
    fn head_symbol_returns_operator_for_list_form() {
        // `(defpoint obs :class x)` — the operator is the head symbol.
        let form = Sexp::List(vec![
            Sexp::symbol("defpoint"),
            Sexp::symbol("obs"),
            Sexp::keyword("class"),
            Sexp::symbol("x"),
        ]);
        assert_eq!(form.head_symbol(), Some("defpoint"));
    }

    #[test]
    fn head_symbol_none_for_non_list_shapes() {
        // A bare atom is not an invocation — there is no operator position.
        assert_eq!(Sexp::symbol("foo").head_symbol(), None);
        assert_eq!(Sexp::int(5).head_symbol(), None);
        assert_eq!(Sexp::keyword("k").head_symbol(), None);
        assert_eq!(Sexp::string("s").head_symbol(), None);
        assert_eq!(Sexp::boolean(true).head_symbol(), None);
        assert_eq!(Sexp::float(1.5).head_symbol(), None);
        assert_eq!(Sexp::Nil.head_symbol(), None);
        // Quote-family wrappers are not lists at the outer layer either.
        assert_eq!(Sexp::Quote(Box::new(Sexp::symbol("x"))).head_symbol(), None);
    }

    #[test]
    fn head_symbol_none_for_empty_list() {
        // `()` has no first element to read an operator from.
        assert_eq!(Sexp::List(vec![]).head_symbol(), None);
    }

    #[test]
    fn head_symbol_none_for_non_symbol_head() {
        // A list whose head is present but not a symbol is not a dispatchable
        // invocation — the soft projection yields None (the STRICT sibling
        // `compile_from_sexp` is the one that rejects these loudly).
        assert_eq!(
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]).head_symbol(),
            None
        );
        assert_eq!(
            Sexp::List(vec![Sexp::keyword("kw"), Sexp::symbol("a")]).head_symbol(),
            None
        );
        assert_eq!(
            Sexp::List(vec![Sexp::string("s"), Sexp::symbol("a")]).head_symbol(),
            None
        );
        assert_eq!(
            Sexp::List(vec![
                Sexp::List(vec![Sexp::symbol("nested")]),
                Sexp::symbol("a")
            ])
            .head_symbol(),
            None
        );
        assert_eq!(
            Sexp::List(vec![Sexp::Nil, Sexp::symbol("a")]).head_symbol(),
            None
        );
    }

    #[test]
    fn head_symbol_reads_singleton_list_operator() {
        // `(defcompiler)` — a keyword-only form still has an operator head;
        // this is exactly the arity-gate input compile_named dispatches on
        // before rejecting the missing NAME.
        assert_eq!(
            Sexp::List(vec![Sexp::symbol("defcompiler")]).head_symbol(),
            Some("defcompiler")
        );
    }

    #[test]
    fn head_symbol_borrows_the_actual_head_string() {
        // The returned &str borrows the head atom's contents verbatim — no
        // copy, no normalization. Pin that a multi-segment symbol round-trips
        // unchanged so the dispatch comparison against `T::KEYWORD` is exact.
        let form = Sexp::List(vec![Sexp::symbol("defalert-policy"), Sexp::symbol("p")]);
        assert_eq!(form.head_symbol(), Some("defalert-policy"));
    }

    // ── as_call: the call-form decomposition ────────────────────────────
    //
    // `as_call` pairs `head_symbol` (the operator projection) with the
    // argument tail every dispatch site reads right after matching the
    // operator — `Some((op, &args))` for a symbol-headed list, `None` for
    // everything else. It lifts the `as_list()`-for-the-tail +
    // `head_symbol()`-for-the-operator pairing that recurred at the three
    // soft-dispatch sites (compile.rs `compile_typed` + `compile_named_
    // from_forms`, macro_expand.rs `Expander::expand`) into ONE match.
    // `head_symbol` now delegates to it, so the `as_list()?.first()?.
    // as_symbol()` chain lives in exactly one place. These tests pin the
    // decomposition's contract directly; the existing dispatch tests in
    // compile.rs / macro_expand.rs are the path-uniformity guards proving
    // the three sites route through it without behavior drift.

    #[test]
    fn as_call_decomposes_list_form_into_operator_and_args() {
        // `(defpoint obs :class x)` — the operator is the head symbol and
        // the args are everything after it.
        let args = [
            Sexp::symbol("obs"),
            Sexp::keyword("class"),
            Sexp::symbol("x"),
        ];
        let form = Sexp::List(
            std::iter::once(Sexp::symbol("defpoint"))
                .chain(args.iter().cloned())
                .collect(),
        );
        assert_eq!(form.as_call(), Some(("defpoint", &args[..])));
    }

    #[test]
    fn as_call_none_for_non_call_shapes() {
        // Every shape `head_symbol` rejects, `as_call` rejects identically:
        // non-lists, the empty list, and non-symbol heads have no operator
        // to apply, hence no call decomposition.
        assert_eq!(Sexp::symbol("foo").as_call(), None);
        assert_eq!(Sexp::int(5).as_call(), None);
        assert_eq!(Sexp::keyword("k").as_call(), None);
        assert_eq!(Sexp::string("s").as_call(), None);
        assert_eq!(Sexp::Nil.as_call(), None);
        assert_eq!(Sexp::Quote(Box::new(Sexp::symbol("x"))).as_call(), None);
        assert_eq!(Sexp::List(vec![]).as_call(), None);
        assert_eq!(
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]).as_call(),
            None
        );
        assert_eq!(
            Sexp::List(vec![Sexp::keyword("kw"), Sexp::symbol("a")]).as_call(),
            None
        );
    }

    #[test]
    fn as_call_yields_empty_args_for_singleton_list() {
        // `(defcompiler)` — a keyword-only form decomposes to its operator
        // with an EMPTY argument tail. This is exactly the arity-gate input
        // `compile_named_from_forms` dispatches on before rejecting the
        // missing NAME via `rest.split_first()` returning `None`.
        assert_eq!(
            Sexp::List(vec![Sexp::symbol("defcompiler")]).as_call(),
            Some(("defcompiler", &[][..]))
        );
    }

    #[test]
    fn as_call_args_are_exactly_the_tail_after_the_operator() {
        // The args slice borrows `&list[1..]` verbatim — the head is
        // excluded, every following element is included in order.
        let form = Sexp::List(vec![
            Sexp::symbol("defmonitor"),
            Sexp::symbol("cpu"),
            Sexp::keyword("threshold"),
            Sexp::int(90),
        ]);
        let (op, args) = form.as_call().expect("symbol-headed list decomposes");
        assert_eq!(op, "defmonitor");
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], Sexp::symbol("cpu"));
        assert_eq!(args[2], Sexp::int(90));
    }

    #[test]
    fn head_symbol_is_the_operator_projection_of_as_call() {
        // The structural relationship the lift establishes: `head_symbol`
        // is `as_call().map(|(h, _)| h)`. Pin it across every shape so a
        // regression that drifts one query's head-recognition from the
        // other — e.g. `as_call` accepting a keyword head that `head_symbol`
        // still rejects — fails loudly. The two share ONE chain.
        let shapes = [
            Sexp::symbol("foo"),
            Sexp::int(5),
            Sexp::keyword("k"),
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::symbol("defpoint"), Sexp::symbol("p")]),
            Sexp::List(vec![Sexp::symbol("solo")]),
        ];
        for s in &shapes {
            assert_eq!(
                s.head_symbol(),
                s.as_call().map(|(h, _)| h),
                "head_symbol must equal the operator component of as_call for {s}"
            );
        }
    }

    // ── as_call_to: the keyword-typed call decomposition ────────────────
    //
    // `as_call_to(keyword)` answers "is this a call to ONE specific
    // operator, and what are its arguments?" — the keyword-aware sibling
    // of `as_call`. It lifts the `as_call() + head == T::KEYWORD` two-step
    // chain that recurred at the two `compile.rs` dispatch sites
    // (`compile_typed` and `compile_named_from_forms`) into ONE structural
    // query on the Sexp algebra. The tests below pin its contract
    // directly; the existing `compile_*` tests are the path-uniformity
    // guards proving the two production sites route through it without
    // behavior drift.

    #[test]
    fn as_call_to_returns_args_for_matching_head() {
        // `(defmonitor :name "x")` — head is the exact symbol `defmonitor`,
        // so `as_call_to("defmonitor")` returns `Some(args)` with the tail
        // after the head verbatim.
        let form = Sexp::List(vec![
            Sexp::symbol("defmonitor"),
            Sexp::keyword("name"),
            Sexp::string("x"),
        ]);
        let args = form
            .as_call_to("defmonitor")
            .expect("matching head must yield Some(args)");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], Sexp::keyword("name"));
        assert_eq!(args[1], Sexp::string("x"));
    }

    #[test]
    fn as_call_to_returns_none_for_mismatched_head() {
        // `(defmonitor …)` against keyword `"defpoint"` — same form is a
        // call (so `as_call().is_some()`), but the head doesn't equal the
        // requested keyword. `as_call_to` is the keyword-typed projection,
        // so it yields `None` exactly when the head doesn't match. Pin the
        // gate: the two pre-lift inline sites both rejected this case via
        // `if head != T::KEYWORD { continue }` / `if head == T::KEYWORD`,
        // and the lifted primitive must reject identically.
        let form = Sexp::List(vec![
            Sexp::symbol("defmonitor"),
            Sexp::keyword("name"),
            Sexp::string("x"),
        ]);
        assert!(form.as_call().is_some());
        assert_eq!(form.as_call_to("defpoint"), None);
        assert_eq!(form.as_call_to(""), None);
        assert_eq!(form.as_call_to("DEFMONITOR"), None);
    }

    #[test]
    fn as_call_to_yields_empty_args_for_singleton_matching_call() {
        // `(defcompiler)` against keyword `"defcompiler"` — the head
        // matches and the argument tail is the empty slice. Pin the
        // empty-tail posture: this is exactly the input
        // `compile_named_from_forms` dispatches on before rejecting the
        // missing NAME via `rest.split_first()` returning `None`, so the
        // lifted primitive must yield `Some(&[])` here (NOT `None`) so
        // the downstream split-first gate fires structurally.
        let form = Sexp::List(vec![Sexp::symbol("defcompiler")]);
        assert_eq!(form.as_call_to("defcompiler"), Some(&[][..]));
    }

    #[test]
    fn as_call_to_returns_none_for_non_call_shapes() {
        // Every shape `as_call` rejects, `as_call_to` rejects identically
        // regardless of the requested keyword: non-lists, the empty list,
        // and non-symbol heads have no operator to compare to. Pin
        // path-uniformity with the `as_call` sibling so a regression that
        // narrows the keyword-typed projection to admit a shape the bare
        // soft projection rejected (e.g. accepting a keyword head when
        // `keyword` matches the keyword's symbol-string projection) fails
        // here.
        let shapes = [
            Sexp::symbol("foo"),
            Sexp::int(5),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::boolean(true),
            Sexp::float(1.5),
            Sexp::Nil,
            Sexp::Quote(Box::new(Sexp::symbol("foo"))),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::keyword("foo"), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::string("foo"), Sexp::symbol("a")]),
        ];
        for s in &shapes {
            assert_eq!(
                s.as_call_to("foo"),
                None,
                "non-call shape must yield None for any keyword, got Some for {s}"
            );
            assert_eq!(s.as_call_to("anything"), None);
        }
    }

    #[test]
    fn as_call_to_args_borrow_is_same_pointer_as_as_call_tail() {
        // The structural identity binding `as_call_to` to its `as_call`
        // sibling: on the matching-head path, the returned `args` slice IS
        // the same `&[Sexp]` slice `as_call` would return as the tail
        // component. Pin pointer equality so a regression that
        // re-allocates or copies the tail in the keyword-typed projection
        // fails loudly — the soft-projection contract is borrow, not
        // clone, AND `as_call_to` inherits the contract verbatim from
        // `as_call`.
        let form = Sexp::List(vec![
            Sexp::symbol("defmonitor"),
            Sexp::keyword("name"),
            Sexp::string("x"),
        ]);
        let (_, via_as_call) = form.as_call().expect("call shape");
        let via_as_call_to = form
            .as_call_to("defmonitor")
            .expect("matching keyword shape");
        assert!(
            std::ptr::eq(via_as_call.as_ptr(), via_as_call_to.as_ptr()),
            "as_call_to args must borrow the SAME slice as as_call's tail"
        );
        assert_eq!(via_as_call.len(), via_as_call_to.len());
    }

    #[test]
    fn as_call_to_is_the_keyword_typed_projection_of_as_call() {
        // The structural identity the lift establishes:
        //   `as_call_to(k) == as_call().and_then(|(h, args)| (h == k).then_some(args))`
        //   `as_call_to(k).is_some() == (head_symbol() == Some(k))`
        // Pin both across every shape so a regression that drifts the
        // keyword-typed projection from its closed-form definition fails
        // loudly. The three soft-projection primitives — `head_symbol`,
        // `as_call`, `as_call_to` — must agree on operator-position
        // recognition at every shape they share.
        let shapes = [
            Sexp::symbol("foo"),
            Sexp::int(5),
            Sexp::keyword("k"),
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::symbol("defpoint"), Sexp::symbol("p")]),
            Sexp::List(vec![Sexp::symbol("defmonitor"), Sexp::keyword("name")]),
            Sexp::List(vec![Sexp::symbol("solo")]),
        ];
        for s in &shapes {
            for k in ["defpoint", "defmonitor", "solo", "foo", ""] {
                let via_chain = s.as_call().and_then(|(h, args)| (h == k).then_some(args));
                assert_eq!(
                    s.as_call_to(k),
                    via_chain,
                    "as_call_to({k:?}) must equal as_call+filter for {s}"
                );
                assert_eq!(
                    s.as_call_to(k).is_some(),
                    s.head_symbol() == Some(k),
                    "as_call_to({k:?}).is_some() must equal (head_symbol() == Some({k:?})) for {s}"
                );
            }
        }
    }

    // ── as_call_to_any: the typed-decoded call decomposition ────────────
    //
    // `as_call_to_any(decode)` answers "is this a call whose head decodes
    // through `decode`, and what are its arguments?" — the closure-typed
    // sibling of `as_call_to`. It lifts the
    // `as_list() + head_symbol() + decode(head)` three-step chain that
    // recurred at the macro-expander's `macro_def_from` site (the typed
    // `MacroDefHead::from_keyword` dispatch surface) into ONE structural
    // query on the Sexp algebra. The tests below pin its contract
    // directly; the existing macro-expansion tests are the path-
    // uniformity guards proving the production site routes through it
    // without behavior drift.
    //
    // The test classifier `Op::from_keyword` mirrors `MacroDefHead::from_keyword`
    // — a closed-set typed enum projection from a `&str` head — so the
    // tests cover the macro-expander's real consumer shape rather than a
    // synthetic predicate.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Op {
        Quote,
        If,
        Let,
    }
    impl Op {
        fn from_keyword(head: &str) -> Option<Self> {
            match head {
                "quote" => Some(Self::Quote),
                "if" => Some(Self::If),
                "let" => Some(Self::Let),
                _ => None,
            }
        }
    }

    #[test]
    fn as_call_to_any_returns_decoded_head_and_args_for_matching_head() {
        // `(if c t e)` — head `if` decodes to `Op::If`, args are the
        // three-element tail verbatim. Pin both halves of the returned
        // tuple: the decoded typed witness AND the borrowed args slice.
        let form = Sexp::List(vec![
            Sexp::symbol("if"),
            Sexp::symbol("c"),
            Sexp::symbol("t"),
            Sexp::symbol("e"),
        ]);
        let (op, args) = form
            .as_call_to_any(Op::from_keyword)
            .expect("matching head must yield Some((decoded, args))");
        assert_eq!(op, Op::If);
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], Sexp::symbol("c"));
        assert_eq!(args[2], Sexp::symbol("e"));
    }

    #[test]
    fn as_call_to_any_returns_none_when_decoder_rejects_head() {
        // `(defmonitor :name "x")` — head `defmonitor` is a valid symbol
        // (so `as_call().is_some()`), but `Op::from_keyword` rejects it
        // (it's not one of the closed `{quote, if, let}` set). Pin the
        // gate: `as_call_to_any` yields `None` exactly when the decoder
        // rejects the head, mirroring how the pre-lift inline chain in
        // `macro_def_from` returned `Ok(None)` when
        // `MacroDefHead::from_keyword(head_str)` returned `None`.
        let form = Sexp::List(vec![
            Sexp::symbol("defmonitor"),
            Sexp::keyword("name"),
            Sexp::string("x"),
        ]);
        assert!(form.as_call().is_some());
        assert!(form.as_call_to_any(Op::from_keyword).is_none());
    }

    #[test]
    fn as_call_to_any_yields_empty_args_for_singleton_decoded_call() {
        // `(quote)` against the classifier — head decodes to `Op::Quote`
        // and the argument tail is the empty slice. Pin the empty-tail
        // posture: a downstream arity gate (analogous to
        // `if list.len() < 4` inside `macro_def_from`) dispatches on
        // `args.is_empty()` AFTER the decoder accepts the head; the
        // helper must yield `Some((decoded, &[]))` (NOT `None`) so that
        // gate fires structurally.
        let form = Sexp::List(vec![Sexp::symbol("quote")]);
        let (op, args) = form
            .as_call_to_any(Op::from_keyword)
            .expect("singleton matching call must decompose");
        assert_eq!(op, Op::Quote);
        assert_eq!(args.len(), 0);
    }

    #[test]
    fn as_call_to_any_returns_none_for_non_call_shapes() {
        // Every shape `as_call` rejects, `as_call_to_any` rejects
        // identically regardless of the decoder: non-lists, the empty
        // list, and non-symbol heads have no operator string to feed
        // the decoder. Pin path-uniformity with the `as_call` sibling so
        // a regression that admits a non-call shape (e.g. accepting a
        // bare symbol via a permissive decoder) fails here. Pass
        // `Some` for every input to prove the call-shape gate fires
        // BEFORE the decoder runs — the decoder cannot rescue a
        // non-call.
        let shapes = [
            Sexp::symbol("foo"),
            Sexp::int(5),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::boolean(true),
            Sexp::float(1.5),
            Sexp::Nil,
            Sexp::Quote(Box::new(Sexp::symbol("foo"))),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::keyword("foo"), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::string("foo"), Sexp::symbol("a")]),
        ];
        for s in &shapes {
            // The promiscuous decoder accepts every &str head, so the
            // only way to see `None` here is if the call-shape gate
            // rejects the shape upstream of the decoder.
            assert_eq!(
                s.as_call_to_any(|h: &str| Some(h.to_string())),
                None,
                "non-call shape must yield None even for a promiscuous decoder, got Some for {s}"
            );
        }
    }

    #[test]
    fn as_call_to_any_args_borrow_is_same_pointer_as_as_call_tail() {
        // The structural identity binding `as_call_to_any` to its
        // `as_call` sibling: on the decoded path, the returned `args`
        // slice IS the same `&[Sexp]` slice `as_call` would return as
        // the tail component. Pin pointer equality so a regression that
        // re-allocates or copies the tail in the typed-decoded
        // projection fails loudly — the soft-projection contract is
        // borrow, not clone, AND `as_call_to_any` inherits the contract
        // verbatim from `as_call`. Parallel to the
        // `as_call_to_args_borrow_is_same_pointer_as_as_call_tail` pin
        // for `as_call_to`.
        let form = Sexp::List(vec![
            Sexp::symbol("if"),
            Sexp::symbol("c"),
            Sexp::symbol("t"),
        ]);
        let (_, via_as_call) = form.as_call().expect("call shape");
        let (_, via_as_call_to_any) = form
            .as_call_to_any(Op::from_keyword)
            .expect("decoded shape");
        assert!(
            std::ptr::eq(via_as_call.as_ptr(), via_as_call_to_any.as_ptr()),
            "as_call_to_any args must borrow the SAME slice as as_call's tail"
        );
        assert_eq!(via_as_call.len(), via_as_call_to_any.len());
    }

    #[test]
    fn as_call_to_any_is_the_decoded_projection_of_as_call() {
        // The structural identity the lift establishes:
        //   `as_call_to_any(decode) == as_call().and_then(|(h, args)| decode(h).map(|d| (d, args)))`
        //   `as_call_to_any(decode).is_some() == as_call().map_or(false, |(h, _)| decode(h).is_some())`
        // Pin both across every shape so a regression that drifts the
        // typed-decoded projection from its closed-form definition fails
        // loudly. The four soft-projection primitives — `head_symbol`,
        // `as_call`, `as_call_to`, `as_call_to_any` — must agree on
        // operator-position recognition at every shape they share.
        let shapes = [
            Sexp::symbol("foo"),
            Sexp::int(5),
            Sexp::keyword("k"),
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::symbol("if"), Sexp::symbol("c")]),
            Sexp::List(vec![Sexp::symbol("quote"), Sexp::symbol("x")]),
            Sexp::List(vec![Sexp::symbol("let"), Sexp::List(vec![])]),
            Sexp::List(vec![Sexp::symbol("defpoint"), Sexp::symbol("p")]),
            Sexp::List(vec![Sexp::symbol("solo")]),
        ];
        for s in &shapes {
            let via_chain = s
                .as_call()
                .and_then(|(h, args)| Op::from_keyword(h).map(|d| (d, args)));
            assert_eq!(
                s.as_call_to_any(Op::from_keyword),
                via_chain,
                "as_call_to_any(Op::from_keyword) must equal as_call+decode for {s}"
            );
        }
    }

    #[test]
    fn as_call_to_any_subsumes_as_call_to_via_unit_decoder() {
        // The closed-form composition `as_call_to(k) == as_call_to_any
        // (|h| (h == k).then_some(())).map(|(_, a)| a)` (modulo the
        // discarded `()` decoded witness). Pin it across every shape ×
        // keyword pair so a regression that drifts the typed-decoded
        // projection from its single-keyword sibling fails loudly. This
        // makes the family closure: `as_call_to` is the trivial-decoder
        // instance of `as_call_to_any`, and naming both lets each
        // consumer pick the projection that fits its call site.
        let shapes = [
            Sexp::symbol("foo"),
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::symbol("if"), Sexp::symbol("c")]),
            Sexp::List(vec![Sexp::symbol("defpoint"), Sexp::symbol("p")]),
        ];
        for s in &shapes {
            for k in ["if", "defpoint", "let", "foo", "", "DEFPOINT"] {
                let via_unit_decoder = s
                    .as_call_to_any(|h: &str| (h == k).then_some(()))
                    .map(|(_, args)| args);
                assert_eq!(
                    s.as_call_to(k),
                    via_unit_decoder,
                    "as_call_to({k:?}) must equal as_call_to_any+unit-decoder for {s}"
                );
            }
        }
    }

    // ── iter_calls_to: the slice-side projection of as_call_to ──────────
    //
    // `iter_calls_to(forms, keyword)` lifts the per-form projection
    // `as_call_to` onto a `&[Sexp]`, yielding the args tails of every
    // matching form in source order — the substrate's typed-keyword
    // filter over a batch of forms. The two inline `for form in
    // &expanded { if let Some(args) = form.as_call_to(T::KEYWORD) { … } }`
    // walks at the `compile_typed` + `compile_named_from_forms` dispatch
    // sites (compile.rs) collapse to ONE `iter_calls_to(&expanded,
    // T::KEYWORD)` call. Tests pin the slice-side primitive's contract
    // directly; the existing dispatch tests in compile.rs are the
    // path-uniformity guards proving the two consumers route through it
    // without behavior drift.

    #[test]
    fn iter_calls_to_yields_args_for_every_matching_form_in_slice() {
        // Three forms: two match "defmonitor", one matches "defalert".
        // `iter_calls_to("defmonitor")` yields the two matching args
        // slices in source order — the matched forms' tails verbatim,
        // skipping the non-matching `defalert` form silently.
        let forms = vec![
            Sexp::List(vec![
                Sexp::symbol("defmonitor"),
                Sexp::keyword("name"),
                Sexp::string("a"),
            ]),
            Sexp::List(vec![
                Sexp::symbol("defalert"),
                Sexp::keyword("name"),
                Sexp::string("p"),
            ]),
            Sexp::List(vec![
                Sexp::symbol("defmonitor"),
                Sexp::keyword("name"),
                Sexp::string("b"),
            ]),
        ];
        let args: Vec<&[Sexp]> = iter_calls_to(&forms, "defmonitor").collect();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], &[Sexp::keyword("name"), Sexp::string("a")][..]);
        assert_eq!(args[1], &[Sexp::keyword("name"), Sexp::string("b")][..]);
    }

    #[test]
    fn iter_calls_to_skips_every_non_call_shape_silently() {
        // Every shape `as_call_to` rejects, `iter_calls_to` skips: non-
        // lists (atoms across all 6 atom kinds, Nil, quote-family
        // wrapper), the empty list, and non-symbol-head lists. Pin
        // path-uniformity with the per-form sibling: passing ANY keyword
        // against a slice of non-call shapes yields zero items. Closes
        // the soft-projection posture at the slice level — a regression
        // that admits a non-call shape (e.g. accepting a bare symbol
        // whose name matches the keyword) fails here.
        let forms = vec![
            Sexp::symbol("foo"),
            Sexp::int(5),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::boolean(true),
            Sexp::float(1.5),
            Sexp::Nil,
            Sexp::Quote(Box::new(Sexp::symbol("foo"))),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::keyword("foo"), Sexp::symbol("a")]),
        ];
        for k in ["foo", "anything", "", "defpoint"] {
            let args: Vec<&[Sexp]> = iter_calls_to(&forms, k).collect();
            assert!(
                args.is_empty(),
                "non-call slice must yield zero items for keyword {k:?}, got {} items",
                args.len()
            );
        }
    }

    #[test]
    fn iter_calls_to_yields_empty_args_slice_for_singleton_matching_call() {
        // `(defcompiler)` — the head matches and the args tail is the
        // empty slice. Pin the empty-tail posture: `iter_calls_to` must
        // yield `Some(&[])` for the matching singleton (NOT skip it),
        // mirroring `as_call_to`'s contract — the (possibly-empty) args
        // slice on a match, NOT `None` on an empty tail. This is exactly
        // the input `compile_named_from_forms` dispatches on before
        // rejecting the missing NAME via `rest.split_first()`'s `None`.
        let forms = vec![Sexp::List(vec![Sexp::symbol("defcompiler")])];
        let args: Vec<&[Sexp]> = iter_calls_to(&forms, "defcompiler").collect();
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], &[][..]);
    }

    #[test]
    fn iter_calls_to_yields_nothing_for_empty_slice() {
        // An empty forms slice yields zero items regardless of keyword.
        // Pin the slice-side primitive's degenerate boundary: empty in,
        // empty out — the iterator is fused-empty without consulting
        // `as_call_to` at all.
        let forms: Vec<Sexp> = vec![];
        let mut iter = iter_calls_to(&forms, "anything");
        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_calls_to_yields_nothing_when_keyword_matches_no_form() {
        // A slice of valid call forms whose heads none match the
        // requested keyword yields zero items. Pin path-uniformity with
        // the per-form sibling: every form's `as_call_to(missing)` is
        // `None`, so the slice-side iterator yields nothing — the filter
        // fires uniformly across the batch.
        let forms = vec![
            Sexp::List(vec![Sexp::symbol("defmonitor"), Sexp::int(1)]),
            Sexp::List(vec![Sexp::symbol("defalert"), Sexp::int(2)]),
            Sexp::List(vec![Sexp::symbol("defpoint"), Sexp::int(3)]),
        ];
        let args: Vec<&[Sexp]> = iter_calls_to(&forms, "missing").collect();
        assert!(args.is_empty());
    }

    #[test]
    fn iter_calls_to_args_borrow_is_same_pointer_as_per_form_as_call_to_tail() {
        // The structural identity binding `iter_calls_to` to its per-form
        // sibling: each yielded `&[Sexp]` IS the same slice `as_call_to`
        // would return as the tail component for the corresponding form
        // (pinned via `std::ptr::eq` on `as_ptr()`). The soft-projection
        // contract is borrow, not clone, AND `iter_calls_to` inherits the
        // contract verbatim from `as_call_to`. Parallel to the
        // `as_call_to_args_borrow_is_same_pointer_as_as_call_tail` pin
        // for `as_call_to`.
        let forms = vec![Sexp::List(vec![
            Sexp::symbol("defmonitor"),
            Sexp::keyword("name"),
            Sexp::string("a"),
        ])];
        let via_iter: &[Sexp] = iter_calls_to(&forms, "defmonitor")
            .next()
            .expect("one match");
        let via_per_form: &[Sexp] = forms[0].as_call_to("defmonitor").expect("one match");
        assert!(
            std::ptr::eq(via_iter.as_ptr(), via_per_form.as_ptr()),
            "iter_calls_to args must borrow the SAME slice as as_call_to's tail"
        );
        assert_eq!(via_iter.len(), via_per_form.len());
    }

    #[test]
    fn iter_calls_to_is_the_slice_side_projection_of_as_call_to() {
        // The structural identity the lift establishes:
        //   `iter_calls_to(forms, k) == forms.iter().filter_map(|f| f.as_call_to(k))`
        // Pin shape AND ordering AND pointer-identity across mixed inputs
        // and a range of keywords (including matching, non-matching, and
        // edge-case empty/case-mismatched keywords) so a regression that
        // drifts the slice-side projection from its closed-form
        // definition fails loudly. The five soft-projection primitives —
        // `head_symbol`, `as_call`, `as_call_to`, `as_call_to_any`, AND
        // `iter_calls_to` — must agree on operator-position recognition
        // at every shape/slice they share.
        let forms = vec![
            Sexp::symbol("foo"),
            Sexp::List(vec![Sexp::symbol("a"), Sexp::int(1)]),
            Sexp::Nil,
            Sexp::List(vec![Sexp::symbol("a"), Sexp::int(2)]),
            Sexp::int(99),
            Sexp::List(vec![Sexp::symbol("b"), Sexp::int(3)]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::keyword("a"), Sexp::int(4)]),
        ];
        for k in ["a", "b", "c", "", "A"] {
            let via_iter: Vec<&[Sexp]> = iter_calls_to(&forms, k).collect();
            let via_chain: Vec<&[Sexp]> = forms.iter().filter_map(|f| f.as_call_to(k)).collect();
            assert_eq!(
                via_iter.len(),
                via_chain.len(),
                "len drift for keyword {k:?}"
            );
            for (a, b) in via_iter.iter().zip(via_chain.iter()) {
                assert!(
                    std::ptr::eq(a.as_ptr(), b.as_ptr()),
                    "ptr drift at keyword {k:?}: iter slice does not borrow the SAME tail as the per-form chain"
                );
                assert_eq!(a.len(), b.len(), "len drift at keyword {k:?}");
            }
        }
    }

    // ── iter_calls_to_any: the typed-decoded slice-side projection ──────
    //
    // `iter_calls_to_any(forms, decode)` lifts the per-form projection
    // `as_call_to_any` onto a `&[Sexp]`, yielding the `(decoded,
    // &[Sexp])` pair of every form whose head decodes through `decode`
    // — the substrate's typed-decoded filter over a batch of forms,
    // closing the (per-form, slice-side) × (keyword, classifier) 2×2
    // of soft-dispatch primitives at the slice-side classifier corner.
    // The slice-side keyword projection `iter_calls_to` now routes
    // through THIS primitive with a constant-keyword decoder, so the
    // filter-and-fuse implementation lives at ONE site on the slice
    // algebra. Tests pin the slice-side primitive's contract directly
    // alongside the (slice-side keyword, slice-side classifier)
    // composition law that the keyword projection's re-routing
    // establishes.

    #[test]
    fn iter_calls_to_any_yields_decoded_pair_for_every_matching_form_in_slice() {
        // Three forms: two decode through `Op::from_keyword`, one does
        // not (the head `"defalert"` is outside the closed set). The
        // typed-decoded slice walk yields the `(decoded, args)` pair
        // for each matching form in source order, skipping non-decoding
        // forms silently — parallel to how `iter_calls_to` yields ONLY
        // the args slice for keyword-matching forms.
        #[derive(Debug, PartialEq, Eq)]
        enum Op {
            Defmonitor,
            Defpoint,
        }
        impl Op {
            fn from_keyword(h: &str) -> Option<Self> {
                match h {
                    "defmonitor" => Some(Self::Defmonitor),
                    "defpoint" => Some(Self::Defpoint),
                    _ => None,
                }
            }
        }
        let forms = vec![
            Sexp::List(vec![
                Sexp::symbol("defmonitor"),
                Sexp::keyword("name"),
                Sexp::string("a"),
            ]),
            Sexp::List(vec![
                Sexp::symbol("defalert"),
                Sexp::keyword("name"),
                Sexp::string("p"),
            ]),
            Sexp::List(vec![
                Sexp::symbol("defpoint"),
                Sexp::keyword("name"),
                Sexp::string("b"),
            ]),
        ];
        let decoded: Vec<(Op, &[Sexp])> = iter_calls_to_any(&forms, Op::from_keyword).collect();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].0, Op::Defmonitor);
        assert_eq!(
            decoded[0].1,
            &[Sexp::keyword("name"), Sexp::string("a")][..]
        );
        assert_eq!(decoded[1].0, Op::Defpoint);
        assert_eq!(
            decoded[1].1,
            &[Sexp::keyword("name"), Sexp::string("b")][..]
        );
    }

    #[test]
    fn iter_calls_to_any_skips_every_shape_per_form_sibling_rejects() {
        // Every shape `as_call_to_any` rejects, `iter_calls_to_any`
        // skips: non-list shapes, the empty list, non-symbol-head
        // lists, AND lists whose head is a symbol the decoder rejects.
        // Pin the soft-projection contract at the slice level —
        // parallel to `iter_calls_to_skips_every_non_call_shape_silently`
        // but with the decoder rejection axis added so the per-form
        // sibling's two rejection sources (shape-level + decoder-level)
        // both route through the slice-side filter uniformly.
        let forms = vec![
            Sexp::symbol("foo"),
            Sexp::int(5),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::boolean(true),
            Sexp::float(1.5),
            Sexp::Nil,
            Sexp::Quote(Box::new(Sexp::symbol("foo"))),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::keyword("foo"), Sexp::symbol("a")]),
            // A call whose head IS a symbol but the decoder rejects —
            // this is the decoder-level rejection axis the per-form
            // sibling's classifier closure adds beyond the keyword
            // sibling's `head == k` axis.
            Sexp::List(vec![Sexp::symbol("unknown-head"), Sexp::int(1)]),
        ];
        let decoded: Vec<(&'static str, &[Sexp])> =
            iter_calls_to_any(&forms, |_h: &str| None::<&'static str>).collect();
        assert!(
            decoded.is_empty(),
            "non-call / decoder-rejecting slice must yield zero items, got {} items",
            decoded.len()
        );
    }

    #[test]
    fn iter_calls_to_any_yields_empty_args_slice_for_singleton_decoded_call() {
        // `(defcompiler)` decoded through a classifier that accepts
        // the head — the args tail is the empty slice. Pin the
        // empty-tail posture: the typed-decoded slice walk must yield
        // `(decoded, &[])` for the matching singleton (NOT skip it),
        // mirroring the per-form sibling's contract — the
        // (possibly-empty) args slice on a decoded match, NOT `None`
        // on an empty tail. Parallel to
        // `iter_calls_to_yields_empty_args_slice_for_singleton_matching_call`
        // for the keyword sibling and
        // `as_call_to_any_yields_empty_args_for_singleton_decoded_call`
        // for the per-form sibling.
        let forms = vec![Sexp::List(vec![Sexp::symbol("defcompiler")])];
        let decoded: Vec<(&'static str, &[Sexp])> = iter_calls_to_any(&forms, |h: &str| {
            (h == "defcompiler").then_some("defcompiler")
        })
        .collect();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].0, "defcompiler");
        assert_eq!(decoded[0].1, &[][..]);
    }

    #[test]
    fn iter_calls_to_any_yields_nothing_for_empty_slice() {
        // An empty forms slice yields zero items regardless of
        // decoder. Pin the slice-side primitive's degenerate boundary:
        // empty in, empty out — the iterator is fused-empty without
        // consulting `as_call_to_any` at all. The decoder's body must
        // never run (we assert with an explicitly-panicking closure
        // body to prove the fused-empty contract holds before the
        // per-form sibling is consulted). Parallel to
        // `iter_calls_to_yields_nothing_for_empty_slice` for the
        // keyword sibling.
        let forms: Vec<Sexp> = vec![];
        let mut iter = iter_calls_to_any(&forms, |_h: &str| -> Option<()> {
            panic!("decoder must not run on an empty forms slice")
        });
        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_calls_to_any_args_borrow_is_same_pointer_as_per_form_as_call_to_any_tail() {
        // The structural identity binding `iter_calls_to_any` to its
        // per-form sibling: each yielded `&[Sexp]` IS the same slice
        // `as_call_to_any` would return as the tail component for the
        // corresponding form (pinned via `std::ptr::eq` on `as_ptr()`).
        // The soft-projection contract is borrow, not clone, AND
        // `iter_calls_to_any` inherits the contract verbatim from
        // `as_call_to_any`. Parallel to the
        // `iter_calls_to_args_borrow_is_same_pointer_as_per_form_as_call_to_tail`
        // pin for the keyword sibling and the
        // `as_call_to_any_args_borrow_is_same_pointer_as_as_call_tail`
        // pin for the per-form sibling.
        let forms = vec![Sexp::List(vec![
            Sexp::symbol("defmonitor"),
            Sexp::keyword("name"),
            Sexp::string("a"),
        ])];
        let (_, via_iter): (&'static str, &[Sexp]) = iter_calls_to_any(&forms, |h: &str| {
            (h == "defmonitor").then_some("defmonitor")
        })
        .next()
        .expect("one decoded match");
        let (_, via_per_form): (&'static str, &[Sexp]) = forms[0]
            .as_call_to_any(|h: &str| (h == "defmonitor").then_some("defmonitor"))
            .expect("one decoded match");
        assert!(
            std::ptr::eq(via_iter.as_ptr(), via_per_form.as_ptr()),
            "iter_calls_to_any args must borrow the SAME slice as as_call_to_any's tail"
        );
        assert_eq!(via_iter.len(), via_per_form.len());
    }

    #[test]
    fn iter_calls_to_any_is_the_slice_side_projection_of_as_call_to_any() {
        // The structural identity the lift establishes:
        //   iter_calls_to_any(forms, decode) ==
        //       forms.iter().filter_map(|f| f.as_call_to_any(&mut decode))
        // Pin shape AND ordering AND pointer-identity across mixed
        // inputs and a range of decoders (closed-set classifier,
        // always-accept identity, always-reject `None`, partial
        // closed-set on a single head) so a regression that drifts
        // the slice-side projection from its closed-form definition
        // fails loudly. The six soft-projection primitives —
        // `head_symbol`, `as_call`, `as_call_to`, `as_call_to_any`,
        // `iter_calls_to`, AND `iter_calls_to_any` — must agree on
        // operator-position recognition at every shape/slice they
        // share. Parallel to
        // `iter_calls_to_is_the_slice_side_projection_of_as_call_to`
        // for the keyword sibling.
        let forms = vec![
            Sexp::symbol("foo"),
            Sexp::List(vec![Sexp::symbol("a"), Sexp::int(1)]),
            Sexp::Nil,
            Sexp::List(vec![Sexp::symbol("a"), Sexp::int(2)]),
            Sexp::int(99),
            Sexp::List(vec![Sexp::symbol("b"), Sexp::int(3)]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::keyword("a"), Sexp::int(4)]),
            Sexp::List(vec![Sexp::symbol("c"), Sexp::int(5)]),
        ];
        // Closed-set classifier: accept "a" and "c", reject everything
        // else (including the call whose head is "b", to pin the
        // decoder-level rejection axis the keyword sibling does not
        // have).
        let decode_set =
            |h: &str| -> Option<&'static str> { matches!(h, "a" | "c").then_some("ac") };
        let via_iter: Vec<(&'static str, &[Sexp])> =
            iter_calls_to_any(&forms, decode_set).collect();
        let via_chain: Vec<(&'static str, &[Sexp])> = forms
            .iter()
            .filter_map(|f| f.as_call_to_any(decode_set))
            .collect();
        assert_eq!(
            via_iter.len(),
            via_chain.len(),
            "len drift between slice-side and per-form-chain"
        );
        for (a, b) in via_iter.iter().zip(via_chain.iter()) {
            assert_eq!(a.0, b.0, "decoded drift");
            assert!(
                std::ptr::eq(a.1.as_ptr(), b.1.as_ptr()),
                "ptr drift: slice-side does not borrow the SAME tail as the per-form chain"
            );
            assert_eq!(a.1.len(), b.1.len(), "len drift");
        }
    }

    #[test]
    fn iter_calls_to_routes_through_iter_calls_to_any_via_constant_classifier_composition() {
        // The post-lift composition law binding the slice-side
        // keyword projection to the slice-side classifier projection:
        //
        //   iter_calls_to(forms, k) ==
        //       iter_calls_to_any(forms, |h| (h == k).then_some(())).map(|(_, a)| a)
        //
        // Pin shape AND ordering AND pointer-identity across a mixed
        // slice and three representative keywords (matching some,
        // matching none, edge-case empty string) so a regression that
        // drifts `iter_calls_to`'s body away from the typed-decoded
        // routing (e.g. re-inlines the `forms.iter().filter_map(|f|
        // f.as_call_to(keyword))` triple directly) fails loudly even
        // though the rendered slice-of-slices would still match the
        // keyword sibling's output. The pointer-equality axis is
        // load-bearing: a regression that re-derives the filter at
        // both sites would yield byte-identical slices but with
        // distinct closure-capture state, which the
        // pointer-identity check rejects only because both routes
        // share the SAME underlying form-tail borrow chain.
        //
        // Sibling-shape lift to prior-run `UnquoteForm::marker` ⊂
        // `to_quote_form().prefix()` composition (commit 250c001) and
        // `AtomKind::label` ⊂ `sexp_shape().label()` composition
        // (commit 1db697f): both pin the invariant that a typed
        // subset/keyword projection is structurally derived from its
        // parent superset/classifier projection, not a parallel
        // implementation the type system happens to not catch when
        // the two drift.
        let forms = vec![
            Sexp::List(vec![Sexp::symbol("a"), Sexp::int(1)]),
            Sexp::List(vec![Sexp::symbol("b"), Sexp::int(2)]),
            Sexp::List(vec![Sexp::symbol("a"), Sexp::int(3)]),
            Sexp::List(vec![Sexp::symbol("c"), Sexp::int(4)]),
            Sexp::int(99),
        ];
        for k in ["a", "missing", ""] {
            let via_keyword: Vec<&[Sexp]> = iter_calls_to(&forms, k).collect();
            let via_classifier: Vec<&[Sexp]> =
                iter_calls_to_any(&forms, |h: &str| (h == k).then_some(()))
                    .map(|(_, a)| a)
                    .collect();
            assert_eq!(
                via_keyword.len(),
                via_classifier.len(),
                "len drift between keyword projection and classifier composition for k={k:?}"
            );
            for (a, b) in via_keyword.iter().zip(via_classifier.iter()) {
                assert!(
                    std::ptr::eq(a.as_ptr(), b.as_ptr()),
                    "ptr drift at k={k:?}: keyword projection does not share the SAME borrow with the classifier composition"
                );
                assert_eq!(a.len(), b.len(), "len drift at k={k:?}");
            }
        }
    }

    #[test]
    fn iter_calls_to_any_admits_fnmut_classifier_maintaining_state_across_batch_walk() {
        // The slice-side primitive's `FnMut` constraint (vs the
        // per-form sibling's `FnOnce`) admits a classifier that
        // captures mutable state — a counter, a registry cache, a
        // visited-set. Pin the mutable-state contract: a counter
        // closure increments once per matching form (NOT once per
        // call to `f.as_call_to_any(decode)` at every form, since
        // `as_call_to_any` short-circuits before running `decode` on
        // non-list / empty-list / non-symbol-head shapes — only forms
        // that pass the shape gate reach the decoder). The counter's
        // post-walk value pins the exact number of forms that
        // (a) passed the shape gate AND (b) had a head matching the
        // classifier's predicate.
        let forms = vec![
            Sexp::List(vec![Sexp::symbol("a"), Sexp::int(1)]),
            Sexp::int(99), // not a call — `as_call_to_any` short-circuits, decoder never runs
            Sexp::List(vec![Sexp::symbol("a"), Sexp::int(2)]),
            Sexp::List(vec![Sexp::symbol("b"), Sexp::int(3)]),
            Sexp::List(vec![]), // empty list — `as_call_to_any` short-circuits before decoder
            Sexp::List(vec![Sexp::symbol("a"), Sexp::int(4)]),
        ];
        let mut decoder_calls = 0usize;
        // Consume the iterator into a count (NOT a Vec) so the closure
        // capture of `decoder_calls` is dropped at the iterator's end,
        // releasing the mutable borrow before the post-walk assertions
        // re-read `decoder_calls` immutably. A `Vec<((), &[Sexp])>`
        // collection would inherit the closure's `'a` lifetime through
        // the `iter_calls_to_any` return type's unified lifetime
        // parameter and keep the mutable borrow live across the assert
        // (the rust-borrow-checker contract — `decoded`'s lifetime
        // ties to `min(forms, closure)` even though the items
        // themselves only borrow from `forms`).
        let decoded_count = iter_calls_to_any(&forms, |h: &str| {
            decoder_calls += 1;
            (h == "a").then_some(())
        })
        .count();
        // Three forms have head "a"; one form has head "b"; the
        // non-call shapes (Int + empty list) short-circuit before the
        // decoder runs. Decoder is called 4 times (the 4 shape-gate-
        // passing forms); yields 3 matches.
        assert_eq!(
            decoder_calls, 4,
            "decoder must run once per shape-gate-passing form"
        );
        assert_eq!(
            decoded_count, 3,
            "three forms decode through the classifier"
        );
    }

    // ── iter_named_calls_to_any / iter_named_calls_to: slice-side closure
    //    of the (slice × classifier × named) and (slice × constant × named)
    //    corners of the soft-dispatch cube. Pre-lift the named gate
    //    composition (`split_name_slot` over a classifier-decoded args
    //    tail) lived ONLY inside `Expander::expand_and_collect_named_calls_to_any`'s
    //    projection closure — the slice algebra had no named sibling to
    //    the bare [`iter_calls_to_any`]. Post-lift the gate is composed
    //    at the slice level and the Expander surface routes through it
    //    via the SAME `expand_program + iter + map + collect` pipeline
    //    the bare expander surface uses. The tests below pin the slice
    //    primitive's contract DIRECTLY — independent of the Expander
    //    surface — so a classifier-NAME consumer that already holds
    //    expanded forms (a `tatara-check` runner, an LSP buffer walker,
    //    a REPL exhaustive lister) sees the SAME `NamedFormMissingName`
    //    / `NamedFormNonSymbolName` rejection chain the Expander
    //    consumer sees through the surface method.

    #[test]
    fn iter_named_calls_to_any_yields_decoded_triple_for_every_matching_named_form_in_slice() {
        // Closed-set classifier (`Kind::{Foo, Bar}`) that rejects one head
        // out of three on a slice. Every matching form yields a
        // `Result<(Kind, &str, &[Sexp])>` triple in source order; the
        // unmatched form is skipped silently (NOT yielded as `Err`).
        // Fail-before-pass-after: this assert requires the slice
        // primitive to exist AND to yield the typed witness ALONGSIDE
        // the borrowed NAME slot AND the borrowed spec args tail — pre-
        // lift the slice algebra had no named sibling; consumers had
        // to re-derive the four-step `iter_calls_to_any(forms,
        // decode).map(|(d, args)| split_name_slot(args, k).map(|(n,
        // r)| (d, n, r)))` composition at their call site.
        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        enum Kind {
            Foo,
            Bar,
        }
        let forms = crate::reader::read(
            "(deffoo alpha 1) (defbaz gamma 2) (defbar beta 3) (deffoo delta 4)",
        )
        .unwrap();
        let yielded: Vec<(Kind, String, usize)> =
            super::iter_named_calls_to_any(&forms, |h: &str| match h {
                "deffoo" => Some((Kind::Foo, "deffoo")),
                "defbar" => Some((Kind::Bar, "defbar")),
                _ => None,
            })
            .map(|maybe_triple| {
                maybe_triple.map(|(kind, name, args)| (kind, name.to_string(), args.len()))
            })
            .collect::<crate::error::Result<Vec<_>>>()
            .expect("slice-side named-classifier walk must succeed on well-formed forms");
        assert_eq!(
            yielded,
            vec![
                (Kind::Foo, "alpha".to_string(), 1),
                (Kind::Bar, "beta".into(), 1),
                (Kind::Foo, "delta".into(), 1),
            ],
            "iter_named_calls_to_any must yield (decoded, NAME, args_len) in source order, skipping defbaz",
        );
    }

    #[test]
    fn iter_named_calls_to_any_skips_every_non_matching_form_shape_silently() {
        // Soft-projection contract: the slice primitive must skip every
        // shape the classifier rejects — non-list atoms, empty lists,
        // lists with non-symbol heads, lists with unrecognized symbol
        // heads — WITHOUT emitting the `NamedFormMissingName` /
        // `NamedFormNonSymbolName` variants. The named gate fires ONLY
        // for matched-keyword forms whose NAME slot is malformed, NEVER
        // for forms the classifier filtered out first.
        let forms = crate::reader::read(r#":kw "str" 42 () (unrecognized x) (5 y)"#).unwrap();
        let yielded: Vec<()> = super::iter_named_calls_to_any(&forms, |h: &str| {
            (h == "deffoo").then_some(((), "deffoo"))
        })
        .map(|maybe_triple| maybe_triple.map(|_| ()))
        .collect::<crate::error::Result<Vec<_>>>()
        .expect("slice-side named-classifier walk must succeed when zero forms match");
        assert!(
            yielded.is_empty(),
            "slice-side named-classifier walk must yield empty Vec when zero forms match",
        );
    }

    #[test]
    fn iter_named_calls_to_any_emits_named_form_missing_name_for_matched_form_with_no_name_slot() {
        // `(deffoo)` — head matches the classifier (yielding the typed
        // witness AND the classifier-supplied static keyword), but the
        // NAME slot is missing. `split_name_slot`'s arity gate fires
        // inside the slice primitive and emits `NamedFormMissingName {
        // keyword: "deffoo" }`. Pin that the keyword threaded through
        // is the CLASSIFIER-supplied keyword (NOT a hardcoded fallback,
        // NOT the form's head symbol) — a regression that drifted the
        // keyword binding from `decode`'s tuple's second element to the
        // head symbol or to a constant would fail loudly here.
        let forms = crate::reader::read("(deffoo)").unwrap();
        let mut iter = super::iter_named_calls_to_any(&forms, |h: &str| {
            (h == "deffoo").then_some(((), "deffoo"))
        });
        let first = iter.next().expect("matched form must yield an item");
        let err = first.expect_err("matched form with missing NAME must yield Err");
        assert!(
            matches!(
                err,
                crate::error::LispError::NamedFormMissingName { keyword: "deffoo" }
            ),
            "expected NamedFormMissingName {{ keyword: \"deffoo\" }} through slice primitive, got: {err:?}"
        );
    }

    #[test]
    fn iter_named_calls_to_any_emits_named_form_non_symbol_name_for_matched_form_with_int_name() {
        // `(deffoo 42)` — head matches and the NAME-slot arity gate
        // passes, but the NAME slot's shape gate rejects the int
        // literal. Pin that BOTH the classifier-supplied keyword AND
        // the typed `SexpShape::Int` projection flow into the
        // structural variant, identically to how
        // `Expander::expand_and_collect_named_calls_to_any` emits the
        // same variant when its projection composes the same gate.
        let forms = crate::reader::read("(deffoo 42)").unwrap();
        let mut iter = super::iter_named_calls_to_any(&forms, |h: &str| {
            (h == "deffoo").then_some(((), "deffoo"))
        });
        let first = iter.next().expect("matched form must yield an item");
        let err = first.expect_err("matched form with non-symbol NAME must yield Err");
        assert!(
            matches!(
                err,
                crate::error::LispError::NamedFormNonSymbolName {
                    keyword: "deffoo",
                    got: crate::error::SexpShape::Int,
                }
            ),
            "expected NamedFormNonSymbolName {{ keyword: \"deffoo\", got: Int }} through slice primitive, got: {err:?}"
        );
    }

    #[test]
    fn iter_named_calls_to_any_emits_named_form_non_symbol_name_for_matched_form_with_keyword_name()
    {
        // `(deffoo :name)` — sibling shape pin to the int case: a
        // matched form whose NAME slot is a keyword. Together with the
        // int case this closes path-uniformity across distinct
        // non-symbol-or-string `SexpShape` cells at the slice primitive
        // boundary — every consumer routes through the SAME gate
        // composition regardless of the offending shape.
        let forms = crate::reader::read("(deffoo :name)").unwrap();
        let mut iter = super::iter_named_calls_to_any(&forms, |h: &str| {
            (h == "deffoo").then_some(((), "deffoo"))
        });
        let first = iter.next().expect("matched form must yield an item");
        let err = first.expect_err("matched form with keyword NAME must yield Err");
        assert!(
            matches!(
                err,
                crate::error::LispError::NamedFormNonSymbolName {
                    keyword: "deffoo",
                    got: crate::error::SexpShape::Keyword,
                }
            ),
            "expected NamedFormNonSymbolName {{ keyword: \"deffoo\", got: Keyword }} through slice primitive, got: {err:?}"
        );
    }

    #[test]
    fn iter_named_calls_to_any_accepts_string_name_slot_routing_past_the_gate() {
        // `(deffoo "quoted-name" :k v)` — NAME slot is a string
        // literal, which `as_symbol_or_string` (inside `split_name_slot`)
        // accepts alongside symbols. Pin that the slice primitive
        // erases the quote-vs-symbol distinction at the boundary so a
        // consumer sees ONE `&str` shape regardless of authoring
        // choice, matching the equivalent gate in the typed-domain
        // consumer downstream of `named_form_projection<T>`.
        let forms = crate::reader::read(r#"(deffoo "quoted-name" :k "v")"#).unwrap();
        let yielded: Vec<(String, usize)> = super::iter_named_calls_to_any(&forms, |h: &str| {
            (h == "deffoo").then_some(((), "deffoo"))
        })
        .map(|maybe_triple| maybe_triple.map(|(_, name, args)| (name.to_string(), args.len())))
        .collect::<crate::error::Result<Vec<_>>>()
        .expect("string-author NAME slot must route past gate");
        assert_eq!(yielded, vec![("quoted-name".into(), 2)]);
    }

    #[test]
    fn iter_named_calls_to_any_short_circuits_on_first_malformed_name_under_collect() {
        // `(deffoo good 1) (deffoo) (deffoo also-good 2)` — three
        // matched forms; the SECOND has no NAME slot. Pin that
        // `.collect::<Result<Vec<_>, _>>()` short-circuits at the
        // second form (yielding `Err`) WITHOUT yielding the third
        // form's payload. The iterator's lazy iteration combined with
        // `Result::collect`'s short-circuit gives consumers
        // first-failure semantics at the slice boundary, identical to
        // how `Expander::expand_and_collect_named_calls_to_any` already
        // short-circuits.
        let forms = crate::reader::read("(deffoo good 1) (deffoo) (deffoo also-good 2)").unwrap();
        let collected: crate::error::Result<Vec<()>> =
            super::iter_named_calls_to_any(&forms, |h: &str| {
                (h == "deffoo").then_some(((), "deffoo"))
            })
            .map(|maybe_triple| maybe_triple.map(|_| ()))
            .collect();
        let err = collected.expect_err("collect must surface the first failure");
        assert!(
            matches!(
                err,
                crate::error::LispError::NamedFormMissingName { keyword: "deffoo" }
            ),
            "expected NamedFormMissingName at the first malformed NAME, got: {err:?}"
        );
    }

    #[test]
    fn iter_named_calls_to_yields_name_and_spec_args_for_every_matching_form_in_slice() {
        // Constant-keyword sibling of `iter_named_calls_to_any` —
        // discards the `()` typed witness and yields `Result<(&str,
        // &[Sexp])>` per matching form. Pin that the constant-keyword
        // primitive yields the SAME source-ordered set of triples the
        // typed-decoded sibling does on the same source, modulo the
        // discarded typed witness.
        let forms =
            crate::reader::read("(defcheck alpha 1) (other beta) (defcheck gamma 2 3)").unwrap();
        let yielded: Vec<(String, usize)> = super::iter_named_calls_to(&forms, "defcheck")
            .map(|maybe_pair| maybe_pair.map(|(name, args)| (name.to_string(), args.len())))
            .collect::<crate::error::Result<Vec<_>>>()
            .expect("constant-keyword named slice walk must succeed on well-formed forms");
        assert_eq!(
            yielded,
            vec![("alpha".into(), 1), ("gamma".into(), 2)],
            "iter_named_calls_to must yield (NAME, args_len) in source order, skipping unrelated forms",
        );
    }

    #[test]
    fn iter_named_calls_to_routes_through_iter_named_calls_to_any_via_constant_classifier_composition(
    ) {
        // Pin the closed-form composition law binding the constant-
        // keyword named cell to the typed-decoded named-classifier cell
        // at the slice algebra boundary:
        //
        //   iter_named_calls_to(forms, k) ==
        //       iter_named_calls_to_any(forms, |h| (h == k).then_some(((), k)))
        //           .map(|maybe| maybe.map(|(_, n, a)| (n, a)))
        //
        // This makes the typed-decoded named-classifier slice primitive
        // the CANONICAL composition point the constant-keyword sibling
        // routes through — parallel to how `iter_calls_to` /
        // `iter_calls_to_any` bind their composition law on the bare-
        // kwargs axis at the slice level. A regression that drifts ONE
        // sibling's pipeline from the other becomes loudly visible at
        // this assertion.
        let forms =
            crate::reader::read("(defcheck alpha 1) (other beta) (defcheck gamma 2 3)").unwrap();
        let via_constant: Vec<(String, usize)> = super::iter_named_calls_to(&forms, "defcheck")
            .map(|maybe| maybe.map(|(name, args)| (name.to_string(), args.len())))
            .collect::<crate::error::Result<Vec<_>>>()
            .expect("constant-keyword named slice walk must succeed");
        let via_classifier: Vec<(String, usize)> =
            super::iter_named_calls_to_any(&forms, |h: &str| {
                (h == "defcheck").then_some(((), "defcheck"))
            })
            .map(|maybe| maybe.map(|(_, name, args)| (name.to_string(), args.len())))
            .collect::<crate::error::Result<Vec<_>>>()
            .expect("typed-decoded named slice walk with constant-classifier decoder must succeed");
        assert_eq!(
            via_constant, via_classifier,
            "iter_named_calls_to(forms, k) must yield byte-identical payload to iter_named_calls_to_any(forms, |h| (h == k).then_some(((), k))).map(strip)",
        );
    }

    #[test]
    fn iter_named_calls_to_threads_static_keyword_through_missing_variant() {
        // Path-uniformity at the constant-keyword slice primitive
        // boundary: a static `&'static str` keyword threaded into the
        // primitive routes verbatim through the
        // `NamedFormMissingName.keyword` slot when a matched form has
        // no NAME — same threading discipline `split_name_slot` pins at
        // its boundary. Pin three distinct keywords ALL round-trip
        // through the variant's keyword slot.
        for keyword in ["defmonitor", "defalertpolicy", "defcheck"] {
            let src = format!("({keyword})");
            let forms = crate::reader::read(&src).unwrap();
            let mut iter = super::iter_named_calls_to(&forms, keyword);
            let first = iter.next().expect("matched form must yield an item");
            let err = first.expect_err("matched form with missing NAME must yield Err");
            match err {
                crate::error::LispError::NamedFormMissingName { keyword: got } => {
                    assert_eq!(
                        got, keyword,
                        "constant-keyword slice primitive must thread keyword verbatim"
                    );
                }
                other => {
                    panic!("expected NamedFormMissingName for keyword {keyword:?}, got: {other:?}")
                }
            }
        }
    }

    #[test]
    fn iter_named_calls_to_any_admits_fnmut_classifier_maintaining_state_across_batch_walk() {
        // The slice-side typed-decoded named primitive's `FnMut`
        // classifier constraint admits a closure that captures mutable
        // state across the batch walk — counter, registry cache,
        // visited-set — matching the bare-kwargs slice sibling's
        // contract. Pin: a counter-bumping decoder increments once per
        // shape-gate-passing form (NOT once per slice element, since
        // `iter_calls_to_any` short-circuits before the decoder on
        // non-list / empty-list / non-symbol-head shapes), and the
        // post-walk counter equals the number of forms that reached
        // the decoder.
        let forms =
            crate::reader::read("(deffoo a 1) 42 (deffoo b 2) () (defbar c 3) (deffoo d 4)")
                .unwrap();
        let mut decoder_calls = 0usize;
        let yielded: Vec<String> = super::iter_named_calls_to_any(&forms, |h: &str| {
            decoder_calls += 1;
            (h == "deffoo").then_some(((), "deffoo"))
        })
        .map(|maybe| maybe.map(|(_, name, _)| name.to_string()))
        .collect::<crate::error::Result<Vec<_>>>()
        .expect("FnMut classifier dispatch must succeed on well-formed NAME slots");
        // Four (defX …) call forms in the slice pass the shape gate;
        // the int atom and empty list short-circuit before the
        // decoder. Three of the four pass-through-decoder forms
        // dispatch to deffoo; one dispatches to defbar (rejected by
        // the decoder).
        assert_eq!(
            decoder_calls, 4,
            "FnMut decoder must run once per shape-gate-passing form (4 call forms)"
        );
        assert_eq!(
            yielded,
            vec!["a".to_string(), "b".into(), "d".into()],
            "three (deffoo …) forms match; one (defbar …) form is rejected by the decoder",
        );
    }

    #[test]
    fn iter_named_calls_to_any_yields_borrowed_name_and_args_with_form_lifetime() {
        // Pin the borrow-lifetime contract at the slice primitive
        // boundary: the yielded `&'a str` NAME slot and `&'a [Sexp]`
        // spec args tail must borrow from the input slice verbatim —
        // no copy, no allocation. A consumer that holds the iterator's
        // yields alongside the input slice borrow can use the NAME as
        // a lookup key against a registry without paying for a clone.
        let forms = crate::reader::read("(deffoo my-name :k 1 :j 2)").unwrap();
        let mut iter = super::iter_named_calls_to_any(&forms, |h: &str| {
            (h == "deffoo").then_some(((), "deffoo"))
        });
        let (_, name, spec_args) = iter
            .next()
            .expect("matched form must yield an item")
            .expect("well-formed NAME slot must split");
        // Identity-check the NAME borrow: it must point at the same
        // bytes the form's NAME slot symbol borrows from.
        let form_list = forms[0].as_list().expect("form must be a list");
        let form_name = form_list[1]
            .as_symbol()
            .expect("form NAME must be a symbol");
        assert!(
            std::ptr::eq(name.as_ptr(), form_name.as_ptr()),
            "iter_named_calls_to_any must yield the borrowed NAME, NOT an allocated copy"
        );
        // Spec args tail must borrow from the form's tail starting at
        // index 2 (after the NAME slot at index 1).
        assert!(
            std::ptr::eq(spec_args.as_ptr(), &form_list[2] as *const Sexp),
            "iter_named_calls_to_any must yield the borrowed spec args tail, NOT an allocated copy"
        );
        assert_eq!(spec_args.len(), 4);
    }

    // ── as_named_call_to_any / as_named_call_to: per-form × named cell ──
    //
    // The per-form × named corner of the soft-dispatch cube the slice
    // primitive `iter_named_calls_to_any`'s docstring table identified as
    // the documented gap pre-lift ("(composed inline at each named
    // consumer)"). Post-lift the per-form × named row binds to ONE
    // primitive every per-form named consumer composes through, and the
    // slice-side `iter_named_calls_to_any` routes through it via the SAME
    // `forms.iter().filter_map(_)` skeleton `iter_calls_to_any` uses to
    // route through `as_call_to_any`. These tests pin: (a) the three-arm
    // result shape (None for non-match, Some(Ok) for matched-and-
    // well-formed, Some(Err) for matched-but-malformed-NAME) across each
    // distinct shape, (b) the constant-keyword sibling routes through
    // the typed-decoded sibling via constant-classifier composition, (c)
    // the slice-side `iter_named_calls_to_any` IS the
    // `forms.iter().filter_map(|f| f.as_named_call_to_any(_))` projection
    // — the structural identity binding the per-form to the slice row.

    #[test]
    fn as_named_call_to_any_returns_decoded_triple_for_matched_well_formed_form() {
        // `(deffoo my-name :k 1)` — head matches the classifier's `deffoo`
        // arm, NAME slot is the symbol `my-name`, spec args tail is the
        // two-element `:k 1` pair. Pin Some(Ok((decoded, name, args))).
        // Fail-before-pass-after: this assert requires the per-form
        // method to exist AND to thread the typed witness + borrowed
        // NAME + borrowed spec args through ONE projection.
        #[derive(Debug, PartialEq, Eq)]
        enum Kind {
            Foo,
        }
        let form = crate::reader::read("(deffoo my-name :k 1)").unwrap()[0].clone();
        let res = form
            .as_named_call_to_any(|h: &str| match h {
                "deffoo" => Some((Kind::Foo, "deffoo")),
                _ => None,
            })
            .expect("matched head must yield Some(_)")
            .expect("well-formed NAME slot must split");
        assert_eq!(res.0, Kind::Foo);
        assert_eq!(res.1, "my-name");
        assert_eq!(res.2.len(), 2);
    }

    #[test]
    fn as_named_call_to_any_returns_none_when_decoder_rejects_head() {
        // `(unrelated my-name :k 1)` — head is a symbol, but the
        // classifier returns `None`. Pin: the classifier filter face is
        // identical to `as_call_to_any` — `None` short-circuits BEFORE
        // the named gate runs, so a non-matching head with a malformed
        // NAME slot still yields `None`, NOT `Some(Err)`. The soft-
        // filter face is preserved across the cube row.
        let form = crate::reader::read("(unrelated my-name :k 1)").unwrap()[0].clone();
        assert!(form
            .as_named_call_to_any(|h: &str| (h == "deffoo").then_some(((), "deffoo")))
            .is_none());
    }

    #[test]
    fn as_named_call_to_any_returns_none_for_non_call_shapes() {
        // Every shape `as_call_to_any` rejects, `as_named_call_to_any`
        // rejects identically — atom, keyword, empty list, list with
        // non-symbol head. The classifier-filter face is uniformly the
        // soft per-form posture of every other `as_*` method on `Sexp`.
        let shapes: Vec<Sexp> = vec![
            Sexp::int(5),
            Sexp::keyword("deffoo"),
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(1), Sexp::symbol("my-name")]),
        ];
        for s in shapes {
            assert!(
                s.as_named_call_to_any(|h: &str| (h == "deffoo").then_some(((), "deffoo")))
                    .is_none(),
                "non-call shape must yield None for as_named_call_to_any: {s}"
            );
        }
    }

    #[test]
    fn as_named_call_to_any_returns_some_err_for_matched_head_with_no_name_slot() {
        // `(deffoo)` — head matches the classifier's `deffoo` arm but
        // the form is a singleton: NO NAME slot at all. The named gate
        // (`split_name_slot`'s arity gate) fires structurally, yielding
        // `Some(Err(LispError::NamedFormMissingName { keyword: "deffoo" }))`.
        // Pin the strict-gate face on the named row: matched-and-
        // malformed yields the typed structural rejection variant, NOT
        // `None` (which would conflate "not our head" with "our head
        // but missing NAME" and break the cube's strict-vs-soft split).
        let form = crate::reader::read("(deffoo)").unwrap()[0].clone();
        let err = form
            .as_named_call_to_any(|h: &str| (h == "deffoo").then_some(((), "deffoo")))
            .expect("matched head must yield Some(_)")
            .expect_err("missing NAME slot must yield Err");
        assert!(
            matches!(
                err,
                crate::error::LispError::NamedFormMissingName { keyword: "deffoo" }
            ),
            "expected NamedFormMissingName through per-form primitive, got: {err:?}"
        );
    }

    #[test]
    fn as_named_call_to_any_returns_some_err_for_matched_head_with_non_symbol_name() {
        // `(deffoo 5 :k 1)` — head matches but NAME slot is an int
        // literal. The named gate's `as_symbol_or_string` shape gate
        // fires, yielding `Some(Err(LispError::NamedFormNonSymbolName
        // { keyword: "deffoo", got: SexpShape::Int }))`. Pin the strict-
        // gate face for the second structural rejection variant of the
        // named gate AND the typed `SexpShape` projection of the
        // offending slot.
        let form = crate::reader::read("(deffoo 5 :k 1)").unwrap()[0].clone();
        let err = form
            .as_named_call_to_any(|h: &str| (h == "deffoo").then_some(((), "deffoo")))
            .expect("matched head must yield Some(_)")
            .expect_err("non-symbol NAME slot must yield Err");
        assert!(
            matches!(
                err,
                crate::error::LispError::NamedFormNonSymbolName {
                    keyword: "deffoo",
                    got: crate::error::SexpShape::Int,
                }
            ),
            "expected NamedFormNonSymbolName through per-form primitive, got: {err:?}"
        );
    }

    #[test]
    fn as_named_call_to_constant_keyword_routes_through_as_named_call_to_any() {
        // Pin the closed-form composition binding the constant-keyword
        // sibling to the typed-decoded sibling:
        //   as_named_call_to(k) ==
        //     as_named_call_to_any(|h| (h == k).then_some(((), k)))
        //       .map(|res| res.map(|(_, name, rest)| (name, rest)))
        // across every shape in the test fixture set. A regression
        // that re-implements the constant-keyword sibling without
        // routing through the classifier sibling fails this assertion
        // for the matched-and-well-formed AND matched-but-malformed
        // AND non-match arms simultaneously.
        let shapes: Vec<Sexp> = vec![
            crate::reader::read("(defcompiler my-comp :a 1)").unwrap()[0].clone(),
            crate::reader::read("(defcompiler)").unwrap()[0].clone(),
            crate::reader::read("(defcompiler 5)").unwrap()[0].clone(),
            crate::reader::read("(unrelated my-name :k 1)").unwrap()[0].clone(),
            Sexp::int(99),
            Sexp::List(vec![]),
        ];
        // `LispError` is not `PartialEq` (it transitively wraps `Sexp`,
        // which carries an `Atom::Float` whose `f64` is not `Eq`).
        // Compare via formatted-debug strings on the Err arm; Ok arms and
        // None arm compare structurally. The closed-form composition
        // `as_named_call_to(k) == as_named_call_to_any+unit-decoder` is
        // pinned across all three arms.
        for s in &shapes {
            let via_constant = s.as_named_call_to("defcompiler").map(|res| {
                res.map(|(name, rest)| (name.to_string(), rest.len()))
                    .map_err(|e| format!("{e:?}"))
            });
            let via_classifier = s
                .as_named_call_to_any(|h: &str| (h == "defcompiler").then_some(((), "defcompiler")))
                .map(|res| {
                    res.map(|(_, name, rest)| (name.to_string(), rest.len()))
                        .map_err(|e| format!("{e:?}"))
                });
            assert_eq!(
                via_constant, via_classifier,
                "as_named_call_to(k) must equal as_named_call_to_any+unit-decoder for {s}"
            );
        }
    }

    #[test]
    fn iter_named_calls_to_any_is_the_slice_side_filter_map_of_as_named_call_to_any() {
        // Pin the structural identity binding the slice algebra to the
        // per-form algebra:
        //   iter_named_calls_to_any(forms, decode) ==
        //     forms.iter().filter_map(|f| f.as_named_call_to_any(&mut decode))
        // Both sides must yield the SAME Result shape per element in
        // source order — `Ok(triple)` for matched-and-well-formed,
        // `Err(LispError)` for matched-but-malformed, with non-matches
        // skipped by the filter_map. Sibling pin to
        // `iter_calls_to_any_is_the_slice_side_projection_of_as_call_to_any`
        // on the bare-kwargs row — both rows now share ONE
        // `forms.iter().filter_map(_)` skeleton.
        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        enum Kind {
            Foo,
            Bar,
        }
        let src = "(deffoo a :k 1)
                   (other thing)
                   (defbar 7 :j 2)
                   (deffoo b)
                   (defbaz c :m 3)";
        let forms = crate::reader::read(src).unwrap();
        let decode = |h: &str| match h {
            "deffoo" => Some((Kind::Foo, "deffoo")),
            "defbar" => Some((Kind::Bar, "defbar")),
            _ => None,
        };
        let via_iter: Vec<crate::error::Result<(Kind, String, usize)>> =
            super::iter_named_calls_to_any(&forms, decode)
                .map(|res| res.map(|(k, name, args)| (k, name.to_string(), args.len())))
                .collect();
        let via_filter_map: Vec<crate::error::Result<(Kind, String, usize)>> = forms
            .iter()
            .filter_map(|f| f.as_named_call_to_any(decode))
            .map(|res| res.map(|(k, name, args)| (k, name.to_string(), args.len())))
            .collect();
        assert_eq!(
            via_iter.len(),
            via_filter_map.len(),
            "slice-side iter must yield the same number of items as the per-form filter_map",
        );
        for (a, b) in via_iter.iter().zip(via_filter_map.iter()) {
            match (a, b) {
                (Ok(ta), Ok(tb)) => assert_eq!(ta, tb),
                (Err(ea), Err(eb)) => assert_eq!(format!("{ea:?}"), format!("{eb:?}")),
                _ => panic!(
                    "variant drift between slice iter and per-form filter_map: {a:?} vs {b:?}"
                ),
            }
        }
        // Concretely: 3 matched forms (deffoo a, defbar 7, deffoo b);
        // `defbar 7` yields Err (int NAME), other two yield Ok.
        assert_eq!(via_iter.len(), 3);
        assert!(via_iter[0].is_ok());
        assert!(via_iter[1].is_err());
        assert!(via_iter[2].is_ok());
    }

    #[test]
    fn as_named_call_to_any_borrows_name_and_spec_args_from_form_verbatim() {
        // Pin the borrow-lifetime contract at the per-form primitive
        // boundary: the yielded `&str` NAME slot and `&[Sexp]` spec
        // args tail must borrow from the underlying form verbatim — no
        // copy, no allocation. Sibling pin to
        // `iter_named_calls_to_any_yields_borrowed_name_and_args_with_form_lifetime`
        // on the slice algebra — both rows preserve the borrow
        // contract.
        let forms = crate::reader::read("(deffoo my-name :k 1 :j 2)").unwrap();
        let form = &forms[0];
        let (_, name, spec_args) = form
            .as_named_call_to_any(|h: &str| (h == "deffoo").then_some(((), "deffoo")))
            .expect("matched head must yield Some(_)")
            .expect("well-formed NAME slot must split");
        let form_list = form.as_list().expect("form must be a list");
        let form_name = form_list[1]
            .as_symbol()
            .expect("form NAME must be a symbol");
        assert!(
            std::ptr::eq(name.as_ptr(), form_name.as_ptr()),
            "as_named_call_to_any must yield the borrowed NAME, NOT an allocated copy"
        );
        assert!(
            std::ptr::eq(spec_args.as_ptr(), &form_list[2] as *const Sexp),
            "as_named_call_to_any must yield the borrowed spec args tail, NOT an allocated copy"
        );
        assert_eq!(spec_args.len(), 4);
    }

    // ── as_unquote: the unquote-family projection ───────────────────────
    //
    // `as_unquote` lifts the per-callsite `Sexp::Unquote(inner) /
    // Sexp::UnquoteSplice(inner)` arms paired with their `UnquoteForm::
    // Unquote / UnquoteForm::Splice` literals — three sites pre-lift
    // (`compile_node` 2 arms + `substitute` top-level + `substitute`
    // list-inner) — into ONE typed projection on the `Sexp` algebra.
    // These tests pin its contract; the existing path tests in
    // macro_expand.rs are the path-uniformity guards proving the three
    // sites route through it without behavior drift.

    #[test]
    fn as_unquote_decomposes_unquote_into_typed_marker_and_inner() {
        // `,x` — Sexp::Unquote wrapping a symbol. Pin Some((Unquote, &inner)).
        let inner = Sexp::symbol("x");
        let form = Sexp::Unquote(Box::new(inner.clone()));
        let (marker, body) = form
            .as_unquote()
            .expect("`,x` must project to Some((Unquote, _))");
        assert_eq!(marker, UnquoteForm::Unquote);
        assert_eq!(body, &inner);
    }

    #[test]
    fn as_unquote_decomposes_unquote_splice_into_typed_marker_and_inner() {
        // `,@xs` — Sexp::UnquoteSplice wrapping a symbol. Pin
        // Some((Splice, &inner)). Sibling positive control to the Unquote
        // arm: pins BOTH unquote-family variants project to their typed
        // closed-set UnquoteForm pair through ONE projection function.
        let inner = Sexp::symbol("xs");
        let form = Sexp::UnquoteSplice(Box::new(inner.clone()));
        let (marker, body) = form
            .as_unquote()
            .expect("`,@xs` must project to Some((Splice, _))");
        assert_eq!(marker, UnquoteForm::Splice);
        assert_eq!(body, &inner);
    }

    #[test]
    fn as_unquote_none_for_non_unquote_shapes() {
        // Every Sexp shape OUTSIDE the unquote family — atoms, lists, nil,
        // and the OTHER quote-family variants (Quote `'x`, Quasiquote ``x`) —
        // yields None. Pins the projection's exhaustive negative coverage:
        // a regression that drifts the matched-variant set (e.g. a future
        // emitter that projects `'x` into Some((Unquote, _))) would fail
        // here, even before any downstream dispatcher tests fire.
        assert_eq!(Sexp::symbol("foo").as_unquote(), None);
        assert_eq!(Sexp::int(5).as_unquote(), None);
        assert_eq!(Sexp::keyword("k").as_unquote(), None);
        assert_eq!(Sexp::string("s").as_unquote(), None);
        assert_eq!(Sexp::boolean(true).as_unquote(), None);
        assert_eq!(Sexp::float(1.5).as_unquote(), None);
        assert_eq!(Sexp::Nil.as_unquote(), None);
        assert_eq!(Sexp::List(vec![]).as_unquote(), None);
        assert_eq!(
            Sexp::List(vec![Sexp::symbol("a"), Sexp::int(1)]).as_unquote(),
            None
        );
        // `'x` — Quote-family but NOT unquote-family. The closed-set
        // UnquoteForm projection covers only `,` and `,@`; `'` and `` ` ``
        // are siblings that this projection does NOT match.
        assert_eq!(Sexp::Quote(Box::new(Sexp::symbol("x"))).as_unquote(), None);
        assert_eq!(
            Sexp::Quasiquote(Box::new(Sexp::symbol("x"))).as_unquote(),
            None
        );
    }

    #[test]
    fn as_unquote_is_some_iff_matches_unquote_family() {
        // Structural identity: as_unquote().is_some() agrees with the
        // pre-lift `matches!(self, Sexp::Unquote(_) | Sexp::UnquoteSplice(_))`
        // discriminant across the closed Sexp variant set. Sweep every
        // representative Sexp shape and pin equality of the two discriminants
        // — a regression that drifts ONE shape's projection (e.g. adds
        // Quasiquote to the matched set) becomes a typed test failure.
        let shapes: Vec<(&str, Sexp, bool)> = vec![
            ("nil", Sexp::Nil, false),
            ("symbol", Sexp::symbol("x"), false),
            ("keyword", Sexp::keyword("k"), false),
            ("string", Sexp::string("s"), false),
            ("int", Sexp::int(7), false),
            ("float", Sexp::float(2.5), false),
            ("bool", Sexp::boolean(true), false),
            ("empty list", Sexp::List(vec![]), false),
            (
                "non-empty list",
                Sexp::List(vec![Sexp::symbol("op")]),
                false,
            ),
            ("quote", Sexp::Quote(Box::new(Sexp::symbol("x"))), false),
            (
                "quasiquote",
                Sexp::Quasiquote(Box::new(Sexp::symbol("x"))),
                false,
            ),
            ("unquote", Sexp::Unquote(Box::new(Sexp::symbol("x"))), true),
            (
                "unquote-splice",
                Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))),
                true,
            ),
        ];
        for (label, sexp, expect_some) in &shapes {
            let via_proj = sexp.as_unquote().is_some();
            let via_pat = matches!(sexp, Sexp::Unquote(_) | Sexp::UnquoteSplice(_));
            assert_eq!(
                via_proj, *expect_some,
                "as_unquote().is_some() drifted from expected at {label}"
            );
            assert_eq!(
                via_proj, via_pat,
                "as_unquote().is_some() != pre-lift `matches!(_, Unquote | UnquoteSplice)` at {label}"
            );
        }
    }

    #[test]
    fn as_unquote_inner_pointer_is_the_boxed_body() {
        // The returned `&Sexp` borrows the inner box's body verbatim — no
        // clone, no allocation, same lifetime as `&self`. Pin pointer
        // identity: the returned `&Sexp` shares its address with the
        // contents of the original Box, proving no intermediate copy fires
        // at the projection boundary (so consumers walking deeply nested
        // template bodies pay zero allocation per unquote node).
        let inner = Sexp::symbol("payload");
        let boxed = Box::new(inner);
        let inner_ptr: *const Sexp = boxed.as_ref();
        let form = Sexp::Unquote(boxed);
        let (_, body) = form
            .as_unquote()
            .expect("Sexp::Unquote must project to Some");
        assert!(
            std::ptr::eq(body, inner_ptr),
            "as_unquote inner pointer drifted from the boxed body — projection allocates or clones"
        );

        let inner_splice = Sexp::symbol("payload-splice");
        let boxed_splice = Box::new(inner_splice);
        let inner_splice_ptr: *const Sexp = boxed_splice.as_ref();
        let form_splice = Sexp::UnquoteSplice(boxed_splice);
        let (_, body_splice) = form_splice
            .as_unquote()
            .expect("Sexp::UnquoteSplice must project to Some");
        assert!(
            std::ptr::eq(body_splice, inner_splice_ptr),
            "as_unquote inner pointer drifted from the boxed body (splice arm)"
        );
    }

    // ── fmt_float: Display→read round-trip preserves Float identity ──────
    //
    // Rust's stdlib Display for f64 elides trailing `.0` on integral
    // floats — `format!("{}", 1.0_f64) == "1"` — and the substrate's
    // reader tries `i64::parse` before `f64::parse`, so a bare `1` re-reads
    // as `Atom::Int(1)`, NOT `Atom::Float(1.0)`. The Display→read
    // round-trip pre-lift dropped the typed Float identity on every
    // integral float: `Float(1.0)` displayed as `"1"`, re-read as `Int(1)`,
    // and downstream consumers silently typed the slot as Int. The
    // `fmt_float` helper appends `.0` for finite integral values so the
    // round-trip preserves the typed identity. Tests below pin:
    //   (a) Display of `Float(1.0)` is `"1.0"` (fail-before-pass-after);
    //   (b) the Display→read round-trip lands as `Float(1.0)`, NOT
    //       `Int(1)` (the typed-identity preservation contract);
    //   (c) non-integral floats render unchanged through the default
    //       impl (`Float(1.5)` is still `"1.5"`);
    //   (d) negative integral floats inherit the `.0` suffix
    //       (`Float(-2.0)` is `"-2.0"`);
    //   (e) integer Display is unaffected (`Int(1)` is still `"1"`) —
    //       pin path-uniformity so the helper is precisely scoped to
    //       the Float arm.

    #[test]
    fn fmt_float_renders_integral_float_with_trailing_zero() {
        // Fail-before-pass-after: pre-lift `Sexp::float(1.0).to_string()`
        // was `"1"`; post-lift the typed Float identity is preserved by
        // the `.0` suffix.
        assert_eq!(Sexp::float(1.0).to_string(), "1.0");
        assert_eq!(Sexp::float(100.0).to_string(), "100.0");
        assert_eq!(Sexp::float(0.0).to_string(), "0.0");
    }

    #[test]
    fn fmt_float_round_trips_integral_float_through_reader_as_float() {
        // The structural contract the lift establishes: a `Float`
        // serialized via `Display` re-reads as `Float`, NOT `Int`. Pin
        // the round-trip via the reader so a regression that drops the
        // `.0` suffix (or that re-orders the reader's i64/f64 parse
        // attempts to drop the float arm) surfaces here.
        let orig = Sexp::float(1.0);
        let rendered = orig.to_string();
        let forms =
            crate::reader::read(&rendered).expect("integral float must round-trip through reader");
        assert_eq!(forms.len(), 1);
        match &forms[0] {
            Sexp::Atom(Atom::Float(n)) => assert_eq!(*n, 1.0),
            other => panic!("Display->read round-trip dropped the Float identity, got: {other:?}"),
        }
        // Sibling-shape control: a SECOND integral magnitude reinforces
        // that the round-trip preserves the value, not only the type.
        let orig2 = Sexp::float(-42.0);
        let rendered2 = orig2.to_string();
        let forms2 = crate::reader::read(&rendered2)
            .expect("negative integral float must round-trip through reader");
        match &forms2[0] {
            Sexp::Atom(Atom::Float(n)) => assert_eq!(*n, -42.0),
            other => panic!(
                "Display->read of negative integral float dropped Float identity, got: {other:?}"
            ),
        }
    }

    #[test]
    fn fmt_float_preserves_non_integral_float_display() {
        // Path-uniformity: non-integral floats (the case the stdlib impl
        // already handled correctly) must render unchanged. A regression
        // that always-appends `.0` would write `"1.5.0"` and fail
        // here AND fail the reader round-trip below.
        assert_eq!(Sexp::float(1.5).to_string(), "1.5");
        assert_eq!(Sexp::float(0.99).to_string(), "0.99");
        assert_eq!(Sexp::float(-2.75).to_string(), "-2.75");

        // Round-trip control for the non-integral case stays valid: the
        // helper is precisely scoped, so the fractional component is
        // preserved verbatim through the reader.
        let orig = Sexp::float(0.99);
        let forms = crate::reader::read(&orig.to_string())
            .expect("non-integral float must round-trip through reader");
        match &forms[0] {
            Sexp::Atom(Atom::Float(n)) => assert_eq!(*n, 0.99),
            other => panic!("non-integral float round-trip drift, got: {other:?}"),
        }
    }

    // ── QuoteForm + as_quote_form: closed-set quote-family projection ─────
    //
    // `as_quote_form` lifts the per-callsite `Sexp::Quote(inner)
    // / Sexp::Quasiquote(inner) / Sexp::Unquote(inner) /
    // Sexp::UnquoteSplice(inner)` arm-set paired with their
    // per-variant prefix string (`'`, `` ` ``, `,`, `,@`) and
    // discriminator byte (3, 4, 5, 6) into ONE typed projection on
    // the `Sexp` algebra. Three consumers in this file route through
    // it (`Hash for Sexp`, `Display for Sexp`, `Sexp::as_unquote`)
    // so the (Sexp variant, marker, prefix, discriminator) tuple
    // binds at ONE site. Tests below pin:
    //   (a) the projection lands `Some((QuoteForm::*, inner))` for
    //       each of the four wrapper variants AND `None` for every
    //       non-quote-family shape;
    //   (b) `QuoteForm::prefix` returns the canonical reader-token
    //       prefix for each variant — load-bearing for the round-trip
    //       property the `Display`→reader dual encodes;
    //   (c) `QuoteForm::hash_discriminator` returns the same byte
    //       values the pre-lift Hash arms emitted (3, 4, 5, 6) — pin
    //       the cache-key contract so a regression that drifts a
    //       discriminator silently invalidates every cached expansion
    //       fails loudly here;
    //   (d) `QuoteForm::as_unquote_form` projects the 2-of-4 subset
    //       `{Unquote → UnquoteForm::Unquote, UnquoteSplice →
    //       UnquoteForm::Splice}` and yields `None` for `{Quote,
    //       Quasiquote}` — the structural-subset gate the
    //       `Sexp::as_unquote` derivation routes through;
    //   (e) `Sexp::as_unquote` derived from `as_quote_form +
    //       QuoteForm::as_unquote_form` agrees with the pre-lift
    //       arm-based semantic across every Sexp shape — path
    //       uniformity across the subset gate;
    //   (f) the four homoiconic prefixes round-trip through the
    //       reader via `read(format!("{prefix}{inner}"))` into the
    //       matching `Sexp::*` variant — the typed dual of the
    //       reader's prefix dispatch, pinned end-to-end on the four
    //       wrappers (sibling to `fmt_float`'s Float round-trip pin
    //       at the Display→read boundary).

    #[test]
    fn as_quote_form_projects_each_wrapper_variant_to_typed_marker_and_inner() {
        // `'foo` — Sexp::Quote wrapping a symbol. Pin Some((Quote, &inner))
        // with the typed marker AND the borrowed inner body.
        let inner = Sexp::symbol("foo");
        let form = Sexp::Quote(Box::new(inner.clone()));
        let (qf, body) = form.as_quote_form().expect("Sexp::Quote must project");
        assert_eq!(qf, QuoteForm::Quote);
        assert_eq!(body, &inner);

        // `` `foo `` — Sexp::Quasiquote wrapping a symbol.
        let form_qq = Sexp::Quasiquote(Box::new(inner.clone()));
        let (qf_qq, body_qq) = form_qq
            .as_quote_form()
            .expect("Sexp::Quasiquote must project");
        assert_eq!(qf_qq, QuoteForm::Quasiquote);
        assert_eq!(body_qq, &inner);

        // `,foo` — Sexp::Unquote wrapping a symbol.
        let form_u = Sexp::Unquote(Box::new(inner.clone()));
        let (qf_u, body_u) = form_u.as_quote_form().expect("Sexp::Unquote must project");
        assert_eq!(qf_u, QuoteForm::Unquote);
        assert_eq!(body_u, &inner);

        // `,@xs` — Sexp::UnquoteSplice wrapping a symbol.
        let form_us = Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs")));
        let (qf_us, body_us) = form_us
            .as_quote_form()
            .expect("Sexp::UnquoteSplice must project");
        assert_eq!(qf_us, QuoteForm::UnquoteSplice);
        assert_eq!(body_us, &Sexp::symbol("xs"));
    }

    #[test]
    fn as_quote_form_none_for_non_quote_family_shapes() {
        // Every shape OUTSIDE the closed quote-family must project to
        // None: Nil, every Atom variant, and List (empty + populated).
        // Pin the closed-set boundary so a regression that accidentally
        // promotes a non-wrapper variant into the quote family becomes
        // a typed test failure.
        assert_eq!(Sexp::Nil.as_quote_form(), None);
        assert_eq!(Sexp::symbol("x").as_quote_form(), None);
        assert_eq!(Sexp::keyword("k").as_quote_form(), None);
        assert_eq!(Sexp::string("s").as_quote_form(), None);
        assert_eq!(Sexp::int(7).as_quote_form(), None);
        assert_eq!(Sexp::float(2.5).as_quote_form(), None);
        assert_eq!(Sexp::boolean(true).as_quote_form(), None);
        assert_eq!(Sexp::List(vec![]).as_quote_form(), None);
        assert_eq!(
            Sexp::List(vec![Sexp::symbol("op"), Sexp::int(1)]).as_quote_form(),
            None
        );
    }

    #[test]
    fn as_quote_form_inner_pointer_is_the_boxed_body() {
        // The returned `&Sexp` borrows the inner box's body verbatim —
        // no clone, no allocation, same lifetime as `&self`. Pin
        // pointer identity for each of the four wrapper variants so a
        // regression that adds an intermediate copy at the projection
        // boundary surfaces here. Same posture as
        // `as_unquote_inner_pointer_is_the_boxed_body` for its 2-of-4
        // subset.
        let payload = Sexp::symbol("payload");
        let boxed = Box::new(payload);
        let inner_ptr: *const Sexp = boxed.as_ref();
        let form = Sexp::Quote(boxed);
        let (_, body) = form.as_quote_form().expect("Sexp::Quote must project");
        assert!(
            std::ptr::eq(body, inner_ptr),
            "as_quote_form inner pointer drifted from the boxed body — projection allocates or clones"
        );

        let payload_qq = Sexp::symbol("payload-qq");
        let boxed_qq = Box::new(payload_qq);
        let inner_ptr_qq: *const Sexp = boxed_qq.as_ref();
        let form_qq = Sexp::Quasiquote(boxed_qq);
        let (_, body_qq) = form_qq
            .as_quote_form()
            .expect("Sexp::Quasiquote must project");
        assert!(
            std::ptr::eq(body_qq, inner_ptr_qq),
            "as_quote_form inner pointer drifted (quasiquote arm)"
        );
    }

    #[test]
    fn expect_quote_form_projects_each_quote_family_variant_identically_to_as_quote_form() {
        // ASSERTED-TOTAL-FACE CONTRACT: `expect_quote_form` is the
        // asserted-total face of `as_quote_form` — for every quote-family
        // variant it MUST yield the same `(QuoteForm, &Sexp)` projection
        // that `as_quote_form` yields wrapped in `Some`. A regression
        // that drifts the two projections (e.g. a future variant
        // extension that updates `as_quote_form` but forgets to align
        // `expect_quote_form`'s body) surfaces here.
        let inner = Sexp::symbol("payload");
        for variant in [
            Sexp::Quote(Box::new(inner.clone())),
            Sexp::Quasiquote(Box::new(inner.clone())),
            Sexp::Unquote(Box::new(inner.clone())),
            Sexp::UnquoteSplice(Box::new(inner.clone())),
        ] {
            let via_total = variant.expect_quote_form();
            let via_soft = variant.as_quote_form().expect("variant is quote-family");
            assert_eq!(
                via_total.0, via_soft.0,
                "expect_quote_form's QuoteForm drifted from as_quote_form's at {variant}"
            );
            assert!(
                std::ptr::eq(via_total.1, via_soft.1),
                "expect_quote_form's inner pointer drifted from as_quote_form's at {variant}"
            );
        }
    }

    #[test]
    fn expect_quote_form_panics_with_invariant_const_on_non_quote_family_variants() {
        // STATIC-INVARIANT CONTRACT: every non-quote-family variant
        // (Nil, every Atom subkind, List empty + populated) MUST trigger
        // the asserted-total panic with the named
        // `QUOTE_FAMILY_PROJECTION_INVARIANT` message. The const-vs-
        // panic-payload pin catches a future drift where the const is
        // edited without the projection picking it up (or vice versa).
        for variant in [
            Sexp::Nil,
            Sexp::symbol("x"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(2.5),
            Sexp::boolean(true),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("op"), Sexp::int(1)]),
        ] {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = variant.expect_quote_form();
            }));
            let payload = result.expect_err("expect_quote_form must panic on non-quote-family");
            let msg = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&'static str>().copied())
                .expect("panic payload must be a string");
            assert!(
                msg.contains(QUOTE_FAMILY_PROJECTION_INVARIANT),
                "expect_quote_form panic message {msg:?} did not name \
                 QUOTE_FAMILY_PROJECTION_INVARIANT at variant {variant:?}"
            );
        }
    }

    #[test]
    fn quote_family_projection_invariant_const_matches_legacy_inline_literal() {
        // CONST-PIN: pre-lift the panic literal "matched quote-family
        // variant must project to Some via as_quote_form" appeared inline
        // at FIVE production sites (`Hash for Sexp`, `Display for Sexp`,
        // `domain::sexp_shape`, `domain::sexp_to_json`,
        // `interop::iac_forge_tag`). Pin the lifted const to the legacy
        // inline literal bit-for-bit so a regression that drifts the
        // const silently from the historical diagnostic string surfaces
        // here. Sibling shape to `quote_form_hash_discriminator_pins_
        // legacy_cache_key_bytes` for the discriminator-byte algebra.
        assert_eq!(
            QUOTE_FAMILY_PROJECTION_INVARIANT,
            "matched quote-family variant must project to Some via as_quote_form"
        );
    }

    #[test]
    fn quote_form_prefix_pins_canonical_reader_tokens_for_every_variant() {
        // Pin every prefix string load-bearing for the Display→read
        // round-trip. A regression that drifts the prefix (e.g. swaps
        // `'` and `` ` `` between Quote and Quasiquote) silently
        // re-routes every renderer through the wrong variant; this
        // test fails loudly. Sibling-arm sweep so the (variant,
        // prefix) pair stays load-bearing under reordering refactors.
        assert_eq!(QuoteForm::Quote.prefix(), "'");
        assert_eq!(QuoteForm::Quasiquote.prefix(), "`");
        assert_eq!(QuoteForm::Unquote.prefix(), ",");
        assert_eq!(QuoteForm::UnquoteSplice.prefix(), ",@");
    }

    #[test]
    fn quote_form_hash_discriminator_pins_legacy_cache_key_bytes() {
        // CACHE-KEY CONTRACT: pre-lift `Hash for Sexp` used the literal
        // byte values 3/4/5/6 for Quote/Quasiquote/Unquote/UnquoteSplice
        // as the per-variant discriminator. The expansion cache
        // (`Expander::cache`) keys on Hash; ANY change to a
        // discriminator byte silently invalidates every cached
        // expansion across the substrate AND risks collision with the
        // reserved bytes the non-quote-family Hash arms use (0=Nil,
        // 1=Atom, 2=List). Pin the four legacy values explicitly so a
        // regression that re-numbers them surfaces immediately — the
        // `QuoteForm` algebra MUST preserve the prior byte mapping
        // bit-for-bit.
        assert_eq!(QuoteForm::Quote.hash_discriminator(), 3);
        assert_eq!(QuoteForm::Quasiquote.hash_discriminator(), 4);
        assert_eq!(QuoteForm::Unquote.hash_discriminator(), 5);
        assert_eq!(QuoteForm::UnquoteSplice.hash_discriminator(), 6);
    }

    #[test]
    fn quote_form_as_unquote_form_projects_two_of_four_subset() {
        // The structural-subset gate: only `{Unquote, UnquoteSplice}`
        // are template-substitution markers; `{Quote, Quasiquote}` are
        // wrappers whose semantic does NOT include substitution. Pin
        // the 2-of-4 partition so the `Sexp::as_unquote` derivation's
        // closed-set arithmetic stays correct.
        assert_eq!(
            QuoteForm::Unquote.as_unquote_form(),
            Some(UnquoteForm::Unquote)
        );
        assert_eq!(
            QuoteForm::UnquoteSplice.as_unquote_form(),
            Some(UnquoteForm::Splice)
        );
        assert_eq!(QuoteForm::Quote.as_unquote_form(), None);
        assert_eq!(QuoteForm::Quasiquote.as_unquote_form(), None);
    }

    #[test]
    fn quote_form_iac_forge_tag_pins_canonical_lisp_tag_strings_for_every_variant() {
        // CROSS-CRATE CANONICAL-FORM CONTRACT: the four canonical
        // iac-forge tags are load-bearing for inter-crate compatibility
        // — `iac_forge::sexpr::SExpr` consumers (BLAKE3 attestation,
        // render cache) key on the canonical 2-element-list shape
        // `(<tag> <inner>)`. A regression that drifts ONE tag silently
        // invalidates every cached canonical form across the substrate
        // AND mis-collides with the legacy `SexpShape::label` projection
        // that uses the shorter `"unquote-splice"` for the diagnostic
        // surface. Pin the four legacy tag values explicitly so a
        // regression that re-spells them surfaces immediately.
        assert_eq!(QuoteForm::Quote.iac_forge_tag(), "quote");
        assert_eq!(QuoteForm::Quasiquote.iac_forge_tag(), "quasiquote");
        assert_eq!(QuoteForm::Unquote.iac_forge_tag(), "unquote");
        assert_eq!(QuoteForm::UnquoteSplice.iac_forge_tag(), "unquote-splicing");
    }

    #[test]
    fn quote_form_iac_forge_tag_diverges_from_sexp_shape_label_for_unquote_splice() {
        // BOUNDARY-DISTINCT CONTRACT: the iac-forge canonical tag for
        // `UnquoteSplice` is `"unquote-splicing"` (Common Lisp idiom,
        // load-bearing for canonical-form round-trip with the iac-forge
        // ecosystem), distinct from `SexpShape::label`'s shorter
        // `"unquote-splice"` (the substrate's diagnostic label idiom).
        // The two projections key the SAME closed-set on TWO distinct
        // boundaries — pinning the divergence here documents the
        // intent: a future "consolidation" PR that homogenizes them
        // would silently break either the iac-forge canonical-form
        // round-trip OR the operator-facing diagnostic surface. The
        // three other variants (Quote, Quasiquote, Unquote) DO match
        // across both projections — pin that path-uniformity too so a
        // regression that drifts one of the three matched arms surfaces
        // immediately. Sibling-arm sweep so the (variant, tag) AND
        // (variant, label) pairings stay load-bearing under reordering
        // refactors.
        use crate::error::SexpShape;
        assert_eq!(
            QuoteForm::Quote.iac_forge_tag(),
            SexpShape::Quote.label(),
            "quote tag/label agreement"
        );
        assert_eq!(
            QuoteForm::Quasiquote.iac_forge_tag(),
            SexpShape::Quasiquote.label(),
            "quasiquote tag/label agreement"
        );
        assert_eq!(
            QuoteForm::Unquote.iac_forge_tag(),
            SexpShape::Unquote.label(),
            "unquote tag/label agreement"
        );
        // The intentional divergence — load-bearing for the iac-forge
        // canonical form vs the substrate's diagnostic label.
        assert_eq!(QuoteForm::UnquoteSplice.iac_forge_tag(), "unquote-splicing");
        assert_eq!(SexpShape::UnquoteSplice.label(), "unquote-splice");
        assert_ne!(
            QuoteForm::UnquoteSplice.iac_forge_tag(),
            SexpShape::UnquoteSplice.label(),
            "the two projections must disagree at UnquoteSplice — the CL canonical \
             form requires '-splicing' while the substrate's diagnostic label uses \
             the shorter '-splice'; consolidating them would break either side",
        );
    }

    #[test]
    fn quote_form_sexp_shape_pins_canonical_shape_identity_for_every_variant() {
        // CLOSED-SET SHAPE-PROJECTION CONTRACT: each `QuoteForm` variant
        // projects to its matching `SexpShape` variant — load-bearing for
        // the (Sexp variant, SexpShape variant) pairing the substrate's
        // outer-shape projection `domain::sexp_shape` routes through.
        // Sibling-arm sweep so the four pairings stay load-bearing under
        // reordering refactors. A regression that drifts ONE arm (e.g.
        // routes `QuoteForm::Quote` to `SexpShape::Quasiquote`) surfaces
        // here immediately rather than as a silent operator-facing
        // diagnostic drift at every `LispError::TypeMismatch.got` slot
        // for a quote-family witness.
        use crate::error::SexpShape;
        assert_eq!(QuoteForm::Quote.sexp_shape(), SexpShape::Quote);
        assert_eq!(QuoteForm::Quasiquote.sexp_shape(), SexpShape::Quasiquote);
        assert_eq!(QuoteForm::Unquote.sexp_shape(), SexpShape::Unquote);
        assert_eq!(
            QuoteForm::UnquoteSplice.sexp_shape(),
            SexpShape::UnquoteSplice
        );
    }

    #[test]
    fn quote_form_sexp_shape_composes_with_label_for_canonical_short_diagnostic_string() {
        // COMPOSITION-LAW CONTRACT: `qf.sexp_shape().label()` is the
        // canonical short diagnostic string for the quote-family marker
        // — `"quote"`, `"quasiquote"`, `"unquote"`, `"unquote-splice"`.
        // The composition law binds the substrate's typed marker
        // (`QuoteForm`) to its diagnostic surface (`SexpShape::label`)
        // through ONE algebra so a future change to either projection's
        // label (e.g. a substrate-wide rename of `"unquote-splice"` to
        // `"splice"`) rides through the typed composition rather than
        // requiring an inline match at every diagnostic-construction
        // site that previously hand-paired the marker with its label.
        // Pin the short labels here — DISTINCT from the iac-forge tag's
        // `"unquote-splicing"` (load-bearing for the boundary distinction
        // already pinned by
        // `quote_form_iac_forge_tag_diverges_from_sexp_shape_label_for_unquote_splice`).
        assert_eq!(QuoteForm::Quote.sexp_shape().label(), "quote");
        assert_eq!(QuoteForm::Quasiquote.sexp_shape().label(), "quasiquote");
        assert_eq!(QuoteForm::Unquote.sexp_shape().label(), "unquote");
        assert_eq!(
            QuoteForm::UnquoteSplice.sexp_shape().label(),
            "unquote-splice"
        );
    }

    #[test]
    fn quote_form_sexp_shape_paired_with_as_quote_form_preserves_pre_lift_pairing_for_every_sexp() {
        // PATH-UNIFORMITY CONTRACT: the (Sexp variant, SexpShape variant)
        // pairing the pre-lift `sexp_shape` arms encoded inline is now
        // structurally derived via
        // `s.as_quote_form().map(|(qf, _)| qf.sexp_shape())` for every
        // quote-family `Sexp` shape. Pin the derivation against the
        // pre-lift pairing across all four quote-family wrapper variants
        // so a regression that drifts ONE side of the typed algebra
        // (e.g. a `QuoteForm::Quote → SexpShape::Quasiquote` typo, or a
        // `Sexp::as_quote_form` arm that swaps two markers) surfaces
        // immediately. Non-quote-family shapes project to `None` from
        // `as_quote_form`, which the assertion arm skips — the typed
        // closed-set partition is load-bearing for the early-return
        // shape of the lifted `domain::sexp_shape`.
        use crate::error::SexpShape;
        let cases: &[(&str, Sexp, SexpShape)] = &[
            (
                "quote",
                Sexp::Quote(Box::new(Sexp::symbol("x"))),
                SexpShape::Quote,
            ),
            (
                "quasiquote",
                Sexp::Quasiquote(Box::new(Sexp::symbol("x"))),
                SexpShape::Quasiquote,
            ),
            (
                "unquote",
                Sexp::Unquote(Box::new(Sexp::symbol("x"))),
                SexpShape::Unquote,
            ),
            (
                "unquote-splice",
                Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))),
                SexpShape::UnquoteSplice,
            ),
        ];
        for (label, sexp, expected_shape) in cases {
            let (qf, _) = sexp
                .as_quote_form()
                .unwrap_or_else(|| panic!("{label} must project through as_quote_form"));
            assert_eq!(
                qf.sexp_shape(),
                *expected_shape,
                "{label} drifted from typed (QuoteForm, SexpShape) pairing"
            );
        }
    }

    #[test]
    fn as_unquote_derives_from_as_quote_form_composed_with_subset_gate() {
        // Path-uniformity: `Sexp::as_unquote` is now derived from
        // `as_quote_form().and_then(|(qf, inner)| qf.as_unquote_form()
        // .map(|uf| (uf, inner)))`. Pin that the derived semantic
        // agrees with the pre-lift arm-based one across the closed
        // Sexp variant set — every shape's projection through
        // `as_unquote` must equal the manual composition through
        // `as_quote_form` + `QuoteForm::as_unquote_form`. A regression
        // that drifts ONE projection's posture from the composition
        // becomes a typed test failure.
        let shapes: Vec<(&str, Sexp)> = vec![
            ("nil", Sexp::Nil),
            ("symbol", Sexp::symbol("x")),
            ("keyword", Sexp::keyword("k")),
            ("string", Sexp::string("s")),
            ("int", Sexp::int(7)),
            ("float", Sexp::float(2.5)),
            ("bool", Sexp::boolean(true)),
            ("empty list", Sexp::List(vec![])),
            ("non-empty list", Sexp::List(vec![Sexp::symbol("op")])),
            ("quote", Sexp::Quote(Box::new(Sexp::symbol("x")))),
            ("quasiquote", Sexp::Quasiquote(Box::new(Sexp::symbol("x")))),
            ("unquote", Sexp::Unquote(Box::new(Sexp::symbol("x")))),
            (
                "unquote-splice",
                Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))),
            ),
        ];
        for (label, sexp) in &shapes {
            let via_direct = sexp.as_unquote();
            let via_composed = sexp
                .as_quote_form()
                .and_then(|(qf, inner)| qf.as_unquote_form().map(|uf| (uf, inner)));
            assert_eq!(
                via_direct, via_composed,
                "as_unquote drifted from composed as_quote_form+as_unquote_form at {label}"
            );
        }
    }

    #[test]
    fn hash_for_sexp_preserves_legacy_quote_family_discriminator_bytes() {
        // CACHE-KEY CONTRACT (Hash side): pin that the lifted
        // `Hash for Sexp` impl produces byte-identical hashes for the
        // four quote-family variants as the pre-lift implementation.
        // We compute the expected hash via a SECOND hasher that
        // manually drives the pre-lift `<discr>.hash(h); inner.hash(h)`
        // sequence, then compare. A regression that drifts the
        // discriminator OR re-orders the (discr, inner) sequence
        // surfaces here as a hash-value mismatch.
        use std::collections::hash_map::DefaultHasher;
        let inner = Sexp::symbol("payload");
        for (label, sexp, expected_discr) in [
            ("quote", Sexp::Quote(Box::new(inner.clone())), 3u8),
            ("quasiquote", Sexp::Quasiquote(Box::new(inner.clone())), 4u8),
            ("unquote", Sexp::Unquote(Box::new(inner.clone())), 5u8),
            (
                "unquote-splice",
                Sexp::UnquoteSplice(Box::new(inner.clone())),
                6u8,
            ),
        ] {
            let mut via_impl = DefaultHasher::new();
            sexp.hash(&mut via_impl);

            let mut via_legacy = DefaultHasher::new();
            expected_discr.hash(&mut via_legacy);
            inner.hash(&mut via_legacy);

            assert_eq!(
                via_impl.finish(),
                via_legacy.finish(),
                "Hash for Sexp drifted from legacy (discr={expected_discr}, inner) sequence at {label}"
            );
        }
    }

    #[test]
    fn display_for_sexp_renders_each_quote_family_variant_with_canonical_prefix() {
        // Pin the post-lift Display rendering: every wrapper variant
        // renders as `<prefix><inner>` with the prefix sourced from
        // `QuoteForm::prefix`. A regression that drifts the prefix
        // arm-routing (e.g. routes Quote through `` ` `` instead of
        // `'`) fails loudly here. The literal `inner` rendering is
        // the symbol `foo` so the prefix is the only diff between
        // arms — pin path-uniformity across the closed set.
        let inner = Sexp::symbol("foo");
        assert_eq!(Sexp::Quote(Box::new(inner.clone())).to_string(), "'foo");
        assert_eq!(
            Sexp::Quasiquote(Box::new(inner.clone())).to_string(),
            "`foo"
        );
        assert_eq!(Sexp::Unquote(Box::new(inner.clone())).to_string(), ",foo");
        assert_eq!(Sexp::UnquoteSplice(Box::new(inner)).to_string(), ",@foo");
    }

    #[test]
    fn display_for_sexp_round_trips_each_quote_family_variant_through_reader() {
        // ROUND-TRIP CONTRACT: every wrapper variant's Display →
        // reader path produces the matching `Sexp::*` variant. The
        // reader's prefix-dispatch (in `reader::parse`) consumes the
        // canonical `'` / `` ` `` / `,` / `,@` tokens and produces
        // the corresponding wrapper; the Display impl emits the same
        // tokens via `QuoteForm::prefix`. Pin the round-trip
        // end-to-end so a regression that drifts the prefix on
        // either side (Display or reader) fails loudly here. Sibling
        // posture to `fmt_float_round_trips_integral_float_through
        // _reader_as_float` — the Float round-trip pin at the
        // Display→read boundary; this test pins the four
        // quote-family round-trips at the same boundary.
        let inner_body = Sexp::symbol("payload");

        let quote = Sexp::Quote(Box::new(inner_body.clone()));
        let forms = crate::reader::read(&quote.to_string()).expect("quote must round-trip");
        assert_eq!(forms.len(), 1);
        assert_eq!(forms[0], quote);

        let quasiquote = Sexp::Quasiquote(Box::new(inner_body.clone()));
        let forms =
            crate::reader::read(&quasiquote.to_string()).expect("quasiquote must round-trip");
        assert_eq!(forms.len(), 1);
        assert_eq!(forms[0], quasiquote);

        let unquote = Sexp::Unquote(Box::new(inner_body.clone()));
        let forms = crate::reader::read(&unquote.to_string()).expect("unquote must round-trip");
        assert_eq!(forms.len(), 1);
        assert_eq!(forms[0], unquote);

        let splice = Sexp::UnquoteSplice(Box::new(inner_body));
        let forms =
            crate::reader::read(&splice.to_string()).expect("unquote-splice must round-trip");
        assert_eq!(forms.len(), 1);
        assert_eq!(forms[0], splice);
    }

    #[test]
    fn quote_form_wrap_projects_each_typed_marker_into_matching_sexp_wrapper() {
        // CLOSED-SET CONSTRUCTOR CONTRACT: pin that `QuoteForm::wrap` is
        // the structural inverse of `Sexp::as_quote_form` at the
        // marker→wrapper boundary. Every variant of the closed-set
        // `QuoteForm` algebra projects to its matching `Sexp::*` wrapper
        // applied to the supplied inner — `Quote → Sexp::Quote`,
        // `Quasiquote → Sexp::Quasiquote`, `Unquote → Sexp::Unquote`,
        // `UnquoteSplice → Sexp::UnquoteSplice`. A regression that swaps
        // two arms (e.g. `Self::Quote → Sexp::Quasiquote`) type-checks
        // but silently corrupts every consumer that constructs a quote-
        // family Sexp through the projection — fails loudly here.
        // Sibling-arm sweep so the (marker, constructor) pair stays
        // load-bearing under reordering refactors.
        let inner = Sexp::symbol("payload");
        assert_eq!(
            QuoteForm::Quote.wrap(inner.clone()),
            Sexp::Quote(Box::new(inner.clone()))
        );
        assert_eq!(
            QuoteForm::Quasiquote.wrap(inner.clone()),
            Sexp::Quasiquote(Box::new(inner.clone()))
        );
        assert_eq!(
            QuoteForm::Unquote.wrap(inner.clone()),
            Sexp::Unquote(Box::new(inner.clone()))
        );
        assert_eq!(
            QuoteForm::UnquoteSplice.wrap(inner.clone()),
            Sexp::UnquoteSplice(Box::new(inner))
        );
    }

    #[test]
    fn quote_form_wrap_round_trips_through_as_quote_form_for_every_variant() {
        // ROUND-TRIP CONTRACT: pin the structural identity
        // `qf.wrap(inner.clone()).as_quote_form() == Some((qf, &inner))`
        // for every variant of the closed-set `QuoteForm` algebra. This
        // is the canonical law binding the marker→wrapper projection
        // (`wrap`) to its wrapper→marker dual (`as_quote_form`) on the
        // substrate's `Sexp` algebra. A regression that drifts the
        // (marker, constructor) pair on EITHER side — `wrap` routing
        // `Quote` to `Sexp::Quasiquote`, OR `as_quote_form` routing
        // `Sexp::Quote(_)` to `QuoteForm::Quasiquote` — surfaces as a
        // round-trip mismatch here. Sweep all four variants so the
        // round-trip stays load-bearing across the closed set. Same
        // posture as the `display_for_sexp_round_trips_each_quote_family
        // _variant_through_reader` round-trip pin at the Display→read
        // boundary; this test pins the round-trip at the marker→Sexp
        // projection boundary.
        let inner_body = Sexp::symbol("payload");
        for qf in [
            QuoteForm::Quote,
            QuoteForm::Quasiquote,
            QuoteForm::Unquote,
            QuoteForm::UnquoteSplice,
        ] {
            let wrapped = qf.wrap(inner_body.clone());
            let projected = wrapped
                .as_quote_form()
                .expect("wrap output must project back through as_quote_form");
            assert_eq!(
                projected.0, qf,
                "wrap→as_quote_form drifted at marker for variant {qf:?}"
            );
            assert_eq!(
                projected.1, &inner_body,
                "wrap→as_quote_form drifted at inner body for variant {qf:?}"
            );
        }
    }

    #[test]
    fn quote_form_all_is_unique_and_complete() {
        // CLOSED-SET TRUTH-TABLE: pin that `QuoteForm::ALL` carries
        // exactly the four reachable quote-family wrappers — no duplicates,
        // byte-equal coverage of `{Quote, Quasiquote, Unquote, UnquoteSplice}`.
        // The `[Self; 4]` array-literal arity already binds the count at
        // compile time; this test pins the *identity* of each slot so a
        // future re-ordering refactor (e.g. swapping `Unquote` and
        // `UnquoteSplice` positions) that leaves the cardinality intact
        // still fails loudly. Sibling discipline to
        // `unquote_form_all_is_unique_and_complete` (the 2-of-4 subset
        // sibling) and `atom_kind_all_is_unique_and_complete` (the peer
        // atomic-payload axis).
        //
        // The `iter+map+collect+sort_unstable` quadruple this test inlined
        // pre-lift now binds at `<QuoteForm as ClosedSet>::sorted_labels()`
        // — the canonical-ordered candidate-list projection on the trait.
        // Distinctness of the sorted result is covered by
        // `assert_closed_set_well_formed::<QuoteForm>()` (the workspace-wide
        // testkit), so this test reduces to the per-implementor unique
        // payload (the four reader-punctuation literals in lexicographic
        // order — the load-bearing per-enum ground truth the substrate-wide
        // sort lift does NOT subsume).
        assert_eq!(QuoteForm::ALL.len(), 4);
        assert_eq!(
            <QuoteForm as crate::ClosedSet>::sorted_labels(),
            vec!["'", ",", ",@", "`"],
            "QuoteForm::ALL must cover every reachable homoiconic prefix-wrapper"
        );
    }

    #[test]
    fn quote_form_display_matches_prefix_for_every_variant() {
        // DISPLAY-EQUALS-PREFIX CONTRACT: pin that
        // `<QuoteForm as fmt::Display>::fmt` projects through
        // `QuoteForm::prefix` byte-for-byte for every variant in
        // `QuoteForm::ALL`. The Display impl is the canonical rendering
        // surface a future diagnostic annotation (`#[error("... {prefix}")]`
        // shape) threads through; pinning the equality here means a
        // regression that drifts EITHER the Display arm OR the `prefix`
        // arm independently surfaces at this test rather than silently
        // bifurcating the operator-facing rendered marker. Sibling
        // discipline to `unquote_form_display_renders_canonical_marker_
        // for_each_variant` (the 2-of-4 subset sibling) and
        // `atom_kind_display_matches_label_for_every_variant` (the peer
        // atomic-payload axis).
        for qf in QuoteForm::ALL {
            assert_eq!(
                qf.to_string(),
                qf.prefix(),
                "Display rendering for {qf:?} diverged from prefix() projection"
            );
        }
    }

    #[test]
    fn quote_form_prefix_round_trips_through_from_str() {
        // BIDIRECTIONAL ROUND-TRIP: pin the structural identity
        // `qf.prefix().parse() == Ok(qf)` for every variant in
        // `QuoteForm::ALL`. This is the canonical law binding the
        // marker→string projection (`prefix`) to its string→marker dual
        // (`FromStr`). A regression that drifts EITHER side — `prefix`
        // routing `Quote` to `` "`" ``, OR `FromStr` decoding `"'"` to
        // `Quasiquote` — surfaces as a round-trip mismatch here. Sweep
        // all four variants so the round-trip stays load-bearing across
        // the closed set. Same posture as the
        // `unquote_form_marker_round_trips_through_from_str` sibling on
        // the 2-of-4 template-substitution subset axis and
        // `atom_kind_label_round_trips_through_from_str` on the peer
        // atomic-payload axis.
        for qf in QuoteForm::ALL {
            let prefix = qf.prefix();
            let decoded: QuoteForm = prefix
                .parse()
                .expect("canonical prefix must decode through FromStr");
            assert_eq!(
                decoded, qf,
                "FromStr ↔ prefix round-trip drifted for variant {qf:?} (prefix {prefix:?})"
            );
        }
    }

    #[test]
    fn unknown_quote_form_carries_offending_input_verbatim() {
        // TYPED PARSE-FAILURE CONTRACT: pin the exact rendered shape of
        // `UnknownQuoteForm`'s `#[error(...)]` annotation AND the
        // verbatim `.0` field projection — no normalization, no case-
        // folding, no whitespace trimming. The error is part of the
        // substrate-wide `Unknown*` parse-rejection family
        // (`UnknownSexpShape`, `UnknownAtomKind`, `UnknownUnquoteForm`,
        // `UnknownRequestorKind`, `UnknownReceiptKind`, `UnknownPhase`,
        // `UnknownConditionKind`, `UnknownTeardownPolicy`, …) and the
        // joint rendered shape (`"unknown <thing>: {0}"`) is the
        // operator-facing diagnostic idiom every member preserves. A
        // regression that case-folds, trims, or strips the offending
        // input would silently rewrite an operator's literal value at
        // the diagnostic boundary — fails loudly here.
        let offending = "not-a-quote-prefix";
        let err: UnknownQuoteForm = offending
            .parse::<QuoteForm>()
            .expect_err("non-canonical input must reject through FromStr");
        assert_eq!(
            err.0, offending,
            "offending input was not preserved verbatim"
        );
        assert_eq!(
            err.to_string(),
            "unknown quote form: not-a-quote-prefix",
            "Display rendering diverged from the substrate-wide Unknown* idiom"
        );
    }

    #[test]
    fn quote_form_is_well_formed_closed_set() {
        // Structural contract: QuoteForm's four variants are pairwise
        // distinct, round-trip through the trait's `label` ↔
        // `parse_label`, and reject the empty string — the
        // workspace-wide `assert_closed_set_well_formed::<T>()` testkit
        // pinned across every `tatara-process` closed-set implementor
        // (`AllocationPhase`, `RequestorKind`, `ProcessPhase`,
        // `ConditionKind`, `WorkloadKind`, …). The substrate-level
        // assertion runs on the auto-derived `impl ClosedSet for
        // QuoteForm` emitted by `#[derive(tatara_lisp_derive::ClosedSet)]`
        // — a regression that drifts the derive's `make_unknown`
        // delegation, the `via = "prefix"` projection
        // (`"'" / "`" / "," / ",@"`), or the variant listing forced
        // through `Self::ALL` fails-loudly here in isolation from the
        // per-variant truth tables above.
        crate::assert_closed_set_well_formed::<QuoteForm>();
    }

    #[test]
    fn quote_form_from_str_rejects_sexp_shape_labels_on_homoiconic_prefix_axis() {
        // CROSS-AXIS DISJOINTNESS: pin that `QuoteForm::FromStr` decodes
        // the homoiconic punctuation markers `'` / `` ` `` / `,` / `,@`
        // but rejects the `SexpShape` structural-identity vocabulary
        // (`"quote"` / `"quasiquote"` / `"unquote"` / `"unquote-splice"`)
        // AND the `iac_forge_tag` cross-crate canonical-form vocabulary
        // (`"quote"` / `"quasiquote"` / `"unquote"` / `"unquote-splicing"`).
        // The three closed sets project the SAME four `Sexp::*` quote-
        // family constructors on DISTINCT axes — a regression that
        // conflated them would let `"quote".parse::<QuoteForm>()` succeed
        // (silently bifurcating the diagnostic surface) or
        // `"'".parse::<SexpShape>()` succeed (silently colliding the
        // punctuation and structural-identity vocabularies). Sibling
        // discipline to `unquote_form_from_str_rejects_sexp_shape_labels_
        // on_template_marker_axis` (the 2-of-4 subset's matching
        // cross-axis pin).
        use crate::error::SexpShape;
        for shape in [
            SexpShape::Quote,
            SexpShape::Quasiquote,
            SexpShape::Unquote,
            SexpShape::UnquoteSplice,
        ] {
            let label = shape.label();
            assert!(
                label.parse::<QuoteForm>().is_err(),
                "SexpShape label {label:?} unexpectedly decoded through QuoteForm::FromStr — cross-axis vocabulary collision"
            );
        }
        for qf in QuoteForm::ALL {
            let tag = qf.iac_forge_tag();
            assert!(
                tag.parse::<QuoteForm>().is_err(),
                "iac_forge_tag {tag:?} unexpectedly decoded through QuoteForm::FromStr — cross-axis vocabulary collision"
            );
        }
    }

    #[test]
    fn quote_form_from_str_extends_unquote_form_from_str_on_the_2_of_4_subset() {
        // SUBSET-CONTAINMENT CONTRACT: pin that every successful
        // `UnquoteForm::FromStr` input is ALSO a successful
        // `QuoteForm::FromStr` input, AND the resulting variants project
        // to each other through `QuoteForm::as_unquote_form` (the 2-of-4
        // subset gate). This binds the two homoiconic-prefix axes
        // (`UnquoteForm`'s 2-of-2 template-substitution subset and
        // `QuoteForm`'s full 4-of-4 quote-family) at the FromStr
        // boundary: a regression that drifts EITHER FromStr's vocabulary
        // from the other (e.g. `UnquoteForm::FromStr` adding a spelling
        // `","` rejects in `QuoteForm::FromStr` would surface) fails
        // loudly here. Composition law: for every `uf` in
        // `UnquoteForm::ALL`, `uf.marker().parse::<QuoteForm>()` is
        // `Ok(qf)` where `qf.as_unquote_form() == Some(uf)`.
        use crate::error::UnquoteForm;
        for uf in UnquoteForm::ALL {
            let marker = uf.marker();
            let qf: QuoteForm = marker.parse().unwrap_or_else(|_| {
                panic!(
                    "UnquoteForm marker {marker:?} for {uf:?} did not decode through QuoteForm::FromStr — 2-of-4 subset containment violated"
                )
            });
            assert_eq!(
                qf.as_unquote_form(),
                Some(uf),
                "QuoteForm decoded from {marker:?} did not project back to UnquoteForm::{uf:?} via as_unquote_form"
            );
        }
    }

    #[test]
    fn quote_form_wrap_derives_each_arm_to_its_pre_lift_box_new_form() {
        // PATH-UNIFORMITY CONTRACT: pin that `QuoteForm::wrap` is
        // observably equivalent to the pre-lift four-arm reader pattern
        // `Sexp::<Variant>(Box::new(inner))` across every variant of the
        // closed set. The reader's pre-lift parse arms each constructed
        // their corresponding wrapper inline; post-lift the parse routes
        // through `QuoteForm::wrap`. A regression that drifts the
        // projection's allocation posture (e.g. wraps in an extra layer,
        // or skips the `Box::new`) fails loudly here. Companion to the
        // `wrap` projection test above — that test pins the (marker,
        // constructor) pairing; this test pins the structural shape of
        // each wrap output bit-for-bit against the pre-lift inline form.
        let inner = Sexp::List(vec![Sexp::symbol("inner"), Sexp::int(7)]);
        for (qf, expected) in [
            (QuoteForm::Quote, Sexp::Quote(Box::new(inner.clone()))),
            (
                QuoteForm::Quasiquote,
                Sexp::Quasiquote(Box::new(inner.clone())),
            ),
            (QuoteForm::Unquote, Sexp::Unquote(Box::new(inner.clone()))),
            (
                QuoteForm::UnquoteSplice,
                Sexp::UnquoteSplice(Box::new(inner.clone())),
            ),
        ] {
            assert_eq!(
                qf.wrap(inner.clone()),
                expected,
                "wrap drifted from pre-lift Sexp::<Variant>(Box::new(inner)) form for {qf:?}"
            );
        }
    }

    #[test]
    fn fmt_float_leaves_int_display_unchanged() {
        // Path-uniformity sibling: `Atom::Int` Display is unaffected by
        // the `fmt_float` introduction — the helper is wired only into
        // the `Atom::Float` arm of the Display match. A regression that
        // accidentally routes `Atom::Int` through `fmt_float` would
        // render `"1.0"` here and break every consumer that authored an
        // int kwarg expecting the bare-integer rendering.
        assert_eq!(Sexp::int(1).to_string(), "1");
        assert_eq!(Sexp::int(0).to_string(), "0");
        assert_eq!(Sexp::int(-42).to_string(), "-42");
    }

    // ── AtomKind + Atom::kind: closed-set atomic-payload projection ─────
    //
    // `AtomKind` is the closed-set typed discriminator for `Atom`'s six
    // payload variants — `Symbol`, `Keyword`, `Str`, `Int`, `Float`,
    // `Bool`. It is the atomic-payload peer of `QuoteForm` (the four
    // homoiconic prefix wrappers), and the two closed sets together
    // carve every non-Nil non-List arm of `SexpShape`'s twelve-variant
    // closed set via their typed `sexp_shape` projections. Lifting the
    // (Atom variant, byte-discriminator, canonical-label,
    // SexpShape variant) quadruple onto ONE typed algebra collapses:
    //   - `Hash for Atom`'s six byte literals (0/1/2/3/4/5) onto
    //     `AtomKind::hash_discriminator` via `self.kind()` — ONE arm
    //     at the discriminator site;
    //   - `domain::sexp_shape`'s six `Atom::X(_) → SexpShape::X` arms
    //     onto `a.kind().sexp_shape()` — ONE arm at the projection
    //     site;
    //   - any future LSP / REPL / metric-aggregator consumer that
    //     needs to round-trip a rendered diagnostic label back into
    //     the typed discriminator onto `AtomKind::FromStr` — ONE
    //     decode site keyed on `AtomKind::ALL` + `AtomKind::label`.
    //
    // Tests below pin:
    //   (a) `Atom::kind` projects every Atom variant to its typed
    //       discriminator, regardless of inner payload contents;
    //   (b) `AtomKind::ALL` enumerates every variant EXACTLY ONCE;
    //   (c) `AtomKind::label` returns the canonical
    //       lowercase / kebab string for every variant — byte-for-byte
    //       identical to the corresponding `SexpShape::label`;
    //   (d) `Display for AtomKind` delegates to `label`;
    //   (e) `AtomKind::hash_discriminator` returns the same byte
    //       values the pre-lift `Hash for Atom` arms emitted
    //       (0/1/2/3/4/5) — pin the cache-key contract so a
    //       regression that drifts a discriminator silently
    //       invalidates every cached macro expansion fails loudly
    //       here;
    //   (f) `AtomKind::sexp_shape` projects every variant to the
    //       matching `SexpShape` — the typed pairing the
    //       `domain::sexp_shape` collapse relies on;
    //   (g) `AtomKind::FromStr` round-trips every variant through its
    //       label; rejects non-canonical capitalizations, empty input,
    //       and the non-atom `SexpShape` labels (`"nil"`, `"list"`,
    //       `"quote"`, `"quasiquote"`, `"unquote"`, `"unquote-splice"`);
    //   (h) `UnknownAtomKind` carries the offending input verbatim and
    //       renders the `#[error(...)]` annotation byte-exactly;
    //   (i) `Hash for Atom` produces byte-identical hashes for every
    //       atomic variant as the pre-lift implementation — pin the
    //       cache-key contract end-to-end so the post-lift routing
    //       through `AtomKind::hash_discriminator` cannot drift the
    //       cache;
    //   (j) the cross-projection composition law
    //       `crate::domain::sexp_shape(&Sexp::Atom(a)) ==
    //       a.kind().sexp_shape()` holds for every atomic kind.

    #[test]
    fn atom_kind_projects_each_atom_variant_to_typed_marker() {
        // The structural identity `Atom::kind` establishes:
        // `Symbol(_) → AtomKind::Symbol`, `Keyword(_) →
        // AtomKind::Keyword`, etc. Pin every arm with a representative
        // payload + an empty / boundary payload so a regression that
        // matches on the payload rather than the variant identity
        // (e.g. a typo that routes `Str("")` to a different marker
        // than `Str("nonempty")`) surfaces immediately.
        assert_eq!(Atom::Symbol("foo".into()).kind(), AtomKind::Symbol);
        assert_eq!(Atom::Symbol(String::new()).kind(), AtomKind::Symbol);
        assert_eq!(Atom::Keyword("k".into()).kind(), AtomKind::Keyword);
        assert_eq!(Atom::Str("s".into()).kind(), AtomKind::Str);
        assert_eq!(Atom::Str(String::new()).kind(), AtomKind::Str);
        assert_eq!(Atom::Int(0).kind(), AtomKind::Int);
        assert_eq!(Atom::Int(i64::MIN).kind(), AtomKind::Int);
        assert_eq!(Atom::Int(i64::MAX).kind(), AtomKind::Int);
        assert_eq!(Atom::Float(0.0).kind(), AtomKind::Float);
        assert_eq!(Atom::Float(f64::NAN).kind(), AtomKind::Float);
        assert_eq!(Atom::Float(f64::INFINITY).kind(), AtomKind::Float);
        assert_eq!(Atom::Bool(true).kind(), AtomKind::Bool);
        assert_eq!(Atom::Bool(false).kind(), AtomKind::Bool);
    }

    #[test]
    fn atom_kind_all_is_unique_and_complete() {
        // Closed-set posture: `ALL` enumerates every reachable variant
        // EXACTLY ONCE — no duplicates, no omissions. The `[Self; 6]`
        // array literal in the declaration forces the arity at compile
        // time; this test catches the orthogonal failure modes — a
        // future variant added at the type without being added to ALL
        // (silently dropped from every consumer's sweep), or a typo
        // that duplicates an entry (silently double-counted). Same
        // truth-table pinning every sibling closed-set lift in the
        // workspace uses (`SexpShape::ALL`, `RequestorKind::ALL`,
        // `ReceiptKind::ALL`, `ConditionKind::ALL`, `ProcessPhase::ALL`,
        // `ChannelKind::ALL`, …).
        //
        // The `iter+map+collect+sort_unstable` quadruple this test inlined
        // pre-lift now binds at `<AtomKind as ClosedSet>::sorted_labels()`
        // — the canonical-ordered candidate-list projection on the trait.
        // Distinctness of the sorted result is covered by
        // `assert_closed_set_well_formed::<AtomKind>()`, so this test
        // reduces to the per-implementor unique payload (the six diagnostic
        // labels in lexicographic order).
        assert_eq!(AtomKind::ALL.len(), 6);
        assert_eq!(
            <AtomKind as crate::ClosedSet>::sorted_labels(),
            vec!["bool", "float", "int", "keyword", "string", "symbol"],
            "AtomKind::ALL must cover every reachable Atom payload kind"
        );
    }

    #[test]
    fn atom_kind_label_renders_canonical_string_for_every_variant() {
        // Pin every variant's canonical `&'static str` projection — a
        // regression that drifts any label (typo `"sym"` for
        // `"symbol"`, swap of `"int"` ↔ `"float"`, capitalization
        // drift `"String"` for `"string"`, or the `Str → "string"`
        // boundary rename being reversed to a literal `"str"`) fails-
        // loudly here. The six labels are byte-for-byte identical to
        // the corresponding `SexpShape::label` arms so the typed
        // diagnostic vocabulary stays unified across the AtomKind ⊂
        // SexpShape containment.
        assert_eq!(AtomKind::Symbol.label(), "symbol");
        assert_eq!(AtomKind::Keyword.label(), "keyword");
        assert_eq!(AtomKind::Str.label(), "string");
        assert_eq!(AtomKind::Int.label(), "int");
        assert_eq!(AtomKind::Float.label(), "float");
        assert_eq!(AtomKind::Bool.label(), "bool");
    }

    #[test]
    fn atom_kind_label_agrees_with_sexp_shape_label_for_every_atom_arm() {
        // CROSS-PROJECTION VOCABULARY CONTRACT: each `AtomKind`
        // variant's `label()` is byte-for-byte identical to the
        // corresponding `SexpShape` variant's `label()` (after the
        // `Str → String` typed-variant rename which is intentional
        // — the wire vocabulary is `"string"` on both axes). Pin the
        // six-way agreement so a future label rename on EITHER side
        // (a SexpShape `"string"` → `"str"` drift, or an AtomKind
        // `"int"` → `"i64"` drift) fails-loudly here, NOT silently
        // at every cross-axis consumer. The pairing is load-bearing
        // for the typed-projection composition
        // `AtomKind::sexp_shape().label() == AtomKind::label()`.
        //
        // Post-lift this contract is structurally true by composition
        // (`AtomKind::label`'s body IS `self.sexp_shape().label()`),
        // so the cross-axis sweep is a tautology — the regression
        // surface lives at `SexpShape::label`'s atomic arms now,
        // pinned by `atom_kind_label_renders_canonical_string_for_every_variant`
        // (which keys the same six literals through the composition).
        // The sweep stays in place as a structural invariant pin in
        // case a future implementor reverses the lift and re-inlines
        // the per-variant arms here — drift between the two sites
        // would re-emerge and this test catches it.
        for kind in AtomKind::ALL {
            assert_eq!(
                kind.label(),
                kind.sexp_shape().label(),
                "label vocabulary drift between AtomKind::{kind:?} \
                 and its SexpShape projection",
            );
        }
    }

    #[test]
    fn atom_kind_label_routes_through_sexp_shape_label_via_sexp_shape_projection() {
        // ROUTING-PIN CONTRACT: post-lift `AtomKind::label`'s body
        // composes `Self::sexp_shape()` with `SexpShape::label()`
        // verbatim — no inline per-arm literal table. The composition
        // law `AtomKind::label(k) == AtomKind::sexp_shape(k).label()`
        // is structurally true for every `k: AtomKind`; pinning the
        // routing means a regression that re-inlines the six atomic-
        // arm literals here surfaces as a drift between the inline
        // copy and the `SexpShape::label` canonical site rather than
        // surviving silently.
        //
        // Six representative cases — one per variant — walked through
        // the composition manually and through the direct projection,
        // then byte-compared. A drift in EITHER half of the composition
        // (a typo in `Self::sexp_shape`'s match arms swapping
        // `Self::Int → SexpShape::Float`, OR a typo in `SexpShape::label`
        // dropping the `Int → "int"` arm) fails this assertion AND every
        // sibling per-arm assertion in
        // `atom_kind_label_renders_canonical_string_for_every_variant`
        // — but THIS test names the routing axis explicitly so a
        // regression to inline-literal-arms shows up as a failure of
        // the routing pin alongside the per-arm pin.
        //
        // Sibling-lift posture to the prior-run routing pins:
        // `sexp_to_json_object_arm_routes_through_is_kwargs_list_method`
        // (commit 4a11f5b) pins `Sexp::to_json`'s kwargs gate through
        // the lifted predicate. This pin extends the same posture to
        // `AtomKind::label`'s structural routing through the
        // `Self::sexp_shape() ∘ SexpShape::label` composition.
        //
        // Theory anchor: THEORY.md §V.1 — knowable platform; the
        // label-projection routing is a NAMED structural contract
        // pinned alongside the per-arm vocabulary contract, so
        // operators reading the test surface see BOTH the load-bearing
        // identity AND the load-bearing composition. THEORY.md §VI.1
        // — generation over composition; the label projection emerges
        // from the typed pairing rather than per-arm literal discipline,
        // and the routing pin enforces the lift stays in effect.
        for kind in AtomKind::ALL {
            let via_label = kind.label();
            let via_composition = kind.sexp_shape().label();
            assert_eq!(
                via_label, via_composition,
                "AtomKind::{kind:?}::label() must route through \
                 Self::sexp_shape().label() — drift here means the \
                 lift was reverted to inline arms",
            );
            // The pointer-equality check pins the composition produces
            // the SAME `&'static str` (not just a byte-equal copy) for
            // every variant — proof the routing hits ONE static literal
            // site (`SexpShape::label`) rather than a parallel inline
            // table.
            assert!(
                std::ptr::eq(via_label.as_ptr(), via_composition.as_ptr()),
                "AtomKind::{kind:?}::label() must return the SAME \
                 `&'static str` as Self::sexp_shape().label() — \
                 pointer drift means the lift composes through a \
                 parallel literal table rather than routing into the \
                 canonical SexpShape::label site",
            );
        }
    }

    #[test]
    fn atom_kind_display_matches_label_for_every_variant() {
        // Pin Display-equals-label: any future
        // `#[error("... got {got}")]` annotation that threads through
        // this projection projects through Display, and Display
        // delegates to `label()`. A regression that introduces a
        // Display impl that deviates from `label()` (e.g. capitalizing
        // one variant) would drift any future diagnostic surface;
        // this test pins the contract. Sibling posture to
        // `sexp_shape_display_matches_label_for_every_variant` in
        // `error.rs`.
        assert_eq!(format!("{}", AtomKind::Symbol), "symbol");
        assert_eq!(format!("{}", AtomKind::Keyword), "keyword");
        assert_eq!(format!("{}", AtomKind::Str), "string");
        assert_eq!(format!("{}", AtomKind::Int), "int");
        assert_eq!(format!("{}", AtomKind::Float), "float");
        assert_eq!(format!("{}", AtomKind::Bool), "bool");
    }

    #[test]
    fn atom_kind_hash_discriminator_pins_legacy_atom_cache_key_bytes() {
        // CACHE-KEY CONTRACT: pre-lift `Hash for Atom` used the literal
        // byte values 0/1/2/3/4/5 for Symbol/Keyword/Str/Int/Float/Bool
        // as the per-variant discriminator. The macro-expansion cache
        // (`Expander::cache`) keys on Hash; ANY change to a
        // discriminator byte silently invalidates every cached
        // expansion across the substrate. Pin the six legacy values
        // explicitly so a regression that re-numbers them surfaces
        // immediately — the `AtomKind` algebra MUST preserve the prior
        // byte mapping bit-for-bit. Sibling posture to
        // `quote_form_hash_discriminator_pins_legacy_cache_key_bytes`
        // on the quote-family axis.
        assert_eq!(AtomKind::Symbol.hash_discriminator(), 0);
        assert_eq!(AtomKind::Keyword.hash_discriminator(), 1);
        assert_eq!(AtomKind::Str.hash_discriminator(), 2);
        assert_eq!(AtomKind::Int.hash_discriminator(), 3);
        assert_eq!(AtomKind::Float.hash_discriminator(), 4);
        assert_eq!(AtomKind::Bool.hash_discriminator(), 5);
    }

    #[test]
    fn atom_kind_hash_discriminator_bytes_are_pairwise_disjoint() {
        // Closed-set injectivity: the six discriminator bytes must
        // partition `{0, 1, 2, 3, 4, 5}` injectively so two distinct
        // `Atom` variants never produce the SAME hash discriminator —
        // a violation here means the cache could conflate two atomic
        // kinds with identical payloads (`Symbol("x")` and `Str("x")`
        // would silently share a cache slot). Sibling pin to
        // `atom_kind_all_is_unique_and_complete` on the label axis.
        let bytes: Vec<u8> = AtomKind::ALL
            .iter()
            .map(|k| k.hash_discriminator())
            .collect();
        let mut sorted = bytes.clone();
        sorted.sort_unstable();
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(
            sorted, deduped,
            "AtomKind hash discriminator bytes must be pairwise disjoint"
        );
        assert_eq!(sorted, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn atom_kind_sexp_shape_pins_canonical_shape_identity_for_every_variant() {
        // CLOSED-SET SHAPE-PROJECTION CONTRACT: each `AtomKind` variant
        // projects to its matching `SexpShape` variant — load-bearing
        // for the (Atom variant, SexpShape variant) pairing the
        // substrate's outer-shape projection `domain::sexp_shape` routes
        // through. Sibling-arm sweep so the six pairings stay
        // load-bearing under reordering refactors. A regression that
        // drifts ONE arm (e.g. routes `AtomKind::Int` to
        // `SexpShape::Float`) surfaces here immediately rather than as
        // a silent operator-facing diagnostic drift at every
        // `LispError::TypeMismatch.got` slot for an atomic witness.
        // Sibling posture to
        // `quote_form_sexp_shape_pins_canonical_shape_identity_for_every_variant`.
        assert_eq!(AtomKind::Symbol.sexp_shape(), SexpShape::Symbol);
        assert_eq!(AtomKind::Keyword.sexp_shape(), SexpShape::Keyword);
        assert_eq!(AtomKind::Str.sexp_shape(), SexpShape::String);
        assert_eq!(AtomKind::Int.sexp_shape(), SexpShape::Int);
        assert_eq!(AtomKind::Float.sexp_shape(), SexpShape::Float);
        assert_eq!(AtomKind::Bool.sexp_shape(), SexpShape::Bool);
    }

    #[test]
    fn atom_kind_label_round_trips_through_from_str() {
        // Bidirectional `label` ↔ `FromStr` contract: for every variant
        // in ALL, `kind.label().parse() == Ok(kind)`. A regression that
        // drifts the (variant, literal) pairing at ONE arm of `label`
        // (typo, capitalization drift) OR at the `FromStr` decode body
        // (off-by-one, missing variant in the sweep) fails-loudly here.
        // The canonical-literal site is singular (`label`) so the
        // round-trip is the only way the typed surface and the
        // rendered diagnostic literal can drift apart — pinning it
        // here means they cannot. Sibling posture to
        // `sexp_shape_label_round_trips_through_from_str`.
        for kind in AtomKind::ALL {
            let parsed: AtomKind = kind
                .label()
                .parse()
                .expect("every ALL variant's label must round-trip through FromStr");
            assert_eq!(
                parsed,
                kind,
                "FromStr({}) must round-trip to the same variant",
                kind.label()
            );
        }
    }

    #[test]
    fn unknown_atom_kind_carries_offending_input_verbatim() {
        // Operator-facing diagnostic contract: the offending input
        // lands in the typed error verbatim — no normalization, no
        // case-folding, no truncation. Pin the exact `#[error(...)]`
        // rendering AND the typed `.0` field projection so a future
        // refactor that normalizes (e.g. `.to_lowercase()`) before
        // building the error or that drops the input fails-loudly
        // here. Symmetric to every sibling `Unknown*` carrier in the
        // workspace.
        let err: UnknownAtomKind = "Symbol".parse::<AtomKind>().expect_err(
            "capitalized `Symbol` must NOT decode — labels are byte-equal case-sensitive",
        );
        assert_eq!(err.0, "Symbol");
        assert_eq!(format!("{err}"), "unknown atom kind: Symbol");

        let err: UnknownAtomKind = "str"
            .parse::<AtomKind>()
            .expect_err("`str` is not a canonical AtomKind label — `string` is");
        assert_eq!(err.0, "str");
        assert_eq!(format!("{err}"), "unknown atom kind: str");

        let err: UnknownAtomKind = ""
            .parse::<AtomKind>()
            .expect_err("empty input must NOT decode to an AtomKind");
        assert_eq!(err.0, "");
        assert_eq!(format!("{err}"), "unknown atom kind: ");
    }

    #[test]
    fn atom_kind_from_str_rejects_non_atom_sexp_shape_labels() {
        // CROSS-AXIS GUARD: `SexpShape::label()`'s vocabulary is the
        // SUPERSET of `AtomKind::label()`'s — every AtomKind label
        // decodes successfully through SexpShape's FromStr to the
        // matching SexpShape variant (because the typed projections
        // agree), but the SIX non-atom SexpShape labels (`"nil"`,
        // `"list"`, `"quote"`, `"quasiquote"`, `"unquote"`,
        // `"unquote-splice"`) MUST reject through AtomKind's FromStr
        // — they have no atomic-kind preimage. A FromStr that
        // silently accepted `"list"` as an AtomKind would corrupt
        // the typed identity downstream of any future diagnostic
        // round-trip. Pin BOTH directions: the six atom labels
        // decode successfully (and to the matching `AtomKind`
        // variant), the six non-atom labels reject.
        assert_eq!("symbol".parse::<AtomKind>().unwrap(), AtomKind::Symbol);
        assert_eq!("keyword".parse::<AtomKind>().unwrap(), AtomKind::Keyword);
        assert_eq!("string".parse::<AtomKind>().unwrap(), AtomKind::Str);
        assert_eq!("int".parse::<AtomKind>().unwrap(), AtomKind::Int);
        assert_eq!("float".parse::<AtomKind>().unwrap(), AtomKind::Float);
        assert_eq!("bool".parse::<AtomKind>().unwrap(), AtomKind::Bool);

        // Non-atom SexpShape labels (the six structural shapes
        // OUTSIDE the AtomKind closed set) must reject.
        for label in [
            "nil",
            "list",
            "quote",
            "quasiquote",
            "unquote",
            "unquote-splice",
        ] {
            assert!(
                label.parse::<AtomKind>().is_err(),
                "non-atom SexpShape label {label:?} must NOT decode to an AtomKind",
            );
        }

        // Sanity: typed peers' labels (`UnquoteForm::marker`'s
        // `,` / `,@` punctuation, `ExpectedKwargShape`'s
        // `"number"` / `"list of strings"` vocabulary) live on
        // different axes and MUST reject too — pin the closed-set
        // boundary.
        for label in [",", ",@", "number", "list of strings", "atom", "Atom"] {
            assert!(
                label.parse::<AtomKind>().is_err(),
                "cross-axis label {label:?} must NOT decode to an AtomKind",
            );
        }
    }

    #[test]
    fn atom_kind_is_well_formed_closed_set() {
        // Structural contract: AtomKind's six variants are pairwise
        // distinct, round-trip through the trait's `label` ↔
        // `parse_label`, and reject the empty string — the
        // workspace-wide `assert_closed_set_well_formed::<T>()` testkit
        // pinned across every `tatara-process` closed-set implementor
        // (`AllocationPhase`, `RequestorKind`, `ProcessPhase`,
        // `ConditionKind`, `WorkloadKind`, …). The substrate-level
        // assertion runs on the auto-derived `impl ClosedSet for
        // AtomKind` emitted by `#[derive(tatara_lisp_derive::ClosedSet)]`
        // — a regression that drifts the derive's `make_unknown`
        // delegation, the `via = "label"` projection, or the variant
        // listing forced through `Self::ALL` fails-loudly here in
        // isolation from the per-variant truth tables above.
        crate::assert_closed_set_well_formed::<AtomKind>();
    }

    #[test]
    fn hash_for_atom_preserves_legacy_discriminator_bytes() {
        // CACHE-KEY CONTRACT (Hash side): pin that the lifted
        // `Hash for Atom` impl produces byte-identical hashes for the
        // six atomic variants as the pre-lift implementation. We
        // compute the expected hash via a SECOND hasher that manually
        // drives the pre-lift `<discr>u8.hash(h); <inner>.hash(h)`
        // sequence (with `Float`'s `to_bits()` projection preserved
        // and `String` payloads hashed via `String::hash`), then
        // compare. A regression that drifts the discriminator OR
        // re-orders the (discr, inner) sequence surfaces here as a
        // hash-value mismatch. Sibling posture to
        // `hash_for_sexp_preserves_legacy_quote_family_discriminator_bytes`
        // on the quote-family axis.
        use std::collections::hash_map::DefaultHasher;

        let payload = String::from("payload");

        // Helper: hash the legacy `<discr>u8.hash(h); <inner>` shape
        // through a fresh DefaultHasher and finish.
        let legacy_hash = |atom: &Atom, expected_discr: u8| -> u64 {
            let mut h = DefaultHasher::new();
            expected_discr.hash(&mut h);
            match atom {
                Atom::Symbol(s) | Atom::Keyword(s) | Atom::Str(s) => s.hash(&mut h),
                Atom::Int(n) => n.hash(&mut h),
                Atom::Float(f) => f.to_bits().hash(&mut h),
                Atom::Bool(b) => b.hash(&mut h),
            }
            h.finish()
        };

        // (label, atom, pre-lift discriminator byte)
        let cases: &[(&str, Atom, u8)] = &[
            ("symbol", Atom::Symbol(payload.clone()), 0u8),
            ("keyword", Atom::Keyword(payload.clone()), 1u8),
            ("str", Atom::Str(payload.clone()), 2u8),
            ("int", Atom::Int(42), 3u8),
            ("float", Atom::Float(1.5), 4u8),
            ("bool-true", Atom::Bool(true), 5u8),
            ("bool-false", Atom::Bool(false), 5u8),
        ];

        for (label, atom, expected_discr) in cases {
            let mut via_impl = DefaultHasher::new();
            atom.hash(&mut via_impl);

            let via_legacy = legacy_hash(atom, *expected_discr);

            assert_eq!(
                via_impl.finish(),
                via_legacy,
                "Hash for Atom drifted from legacy \
                 (discr={expected_discr}, inner) sequence at {label}"
            );
        }
    }

    #[test]
    fn atom_kind_composes_with_domain_sexp_shape_for_every_atomic_arm() {
        // PATH-UNIFORMITY / COMPOSITION-LAW CONTRACT: the substrate's
        // outer-shape projection `domain::sexp_shape` now routes the
        // six atomic arms through `Atom::kind` + `AtomKind::sexp_shape`.
        // Pin that the composed projection produces the SAME
        // `SexpShape` variant that the pre-lift inline six-arm match
        // produced for every `Atom` payload. A regression that drifts
        // ONE arm of either `Atom::kind` (e.g. routes `Atom::Int(_)`
        // through `AtomKind::Float`) or `AtomKind::sexp_shape` (e.g.
        // routes `AtomKind::Symbol` through `SexpShape::Keyword`)
        // surfaces as an immediate inequality between
        // `domain::sexp_shape(&Sexp::Atom(a))` and
        // `a.kind().sexp_shape()` — and since both projections are
        // load-bearing for the diagnostic surface, the test pins both
        // sides of the typed algebra at once. Sibling posture to
        // `quote_form_sexp_shape_paired_with_as_quote_form_preserves_
        // pre_lift_pairing_for_every_sexp` on the quote-family axis.
        let cases: &[(Atom, SexpShape)] = &[
            (Atom::Symbol("x".into()), SexpShape::Symbol),
            (Atom::Keyword("k".into()), SexpShape::Keyword),
            (Atom::Str("s".into()), SexpShape::String),
            (Atom::Int(7), SexpShape::Int),
            (Atom::Float(2.5), SexpShape::Float),
            (Atom::Bool(true), SexpShape::Bool),
        ];
        for (atom, expected_shape) in cases {
            let via_composed = atom.kind().sexp_shape();
            assert_eq!(
                via_composed, *expected_shape,
                "Atom::kind().sexp_shape() drifted for {atom:?}"
            );
            // Cross-projection identity with the public
            // `domain::sexp_shape` projection — pins that the lifted
            // arm routes through `AtomKind` exactly as the inline
            // arms did pre-lift.
            let via_domain = crate::domain::sexp_shape(&Sexp::Atom(atom.clone()));
            assert_eq!(
                via_domain, via_composed,
                "domain::sexp_shape vs Atom::kind().sexp_shape() drift for {atom:?}"
            );
        }
    }

    #[test]
    fn atom_display_renders_each_variant_to_canonical_form() {
        // CANONICAL-RENDERING CONTRACT: pin that the lifted
        // `fmt::Display for Atom` impl produces byte-identical
        // canonical output for the seven atomic variant cases
        // (Bool splits into true/false) as the pre-lift inline
        // sub-arms inside `Display for Sexp`'s atom arm. Sibling-arm
        // sweep so the seven pairings stay load-bearing under
        // reordering refactors. A regression that drifts the Bool
        // spelling (`#t`/`#f` vs Rust's `true`/`false`) — the
        // CLAUDE.md-pinned reader-round-trip invariant — fails
        // loudly here. Direct sibling to `atom_kind_label_renders_
        // canonical_string_for_every_variant` on the diagnostic-
        // label axis: this pins the rendered SOURCE (`#t`), that pins
        // the rendered LABEL (`bool`); the two projections share the
        // closed-set `AtomKind` algebra but render to distinct
        // surfaces (source vs diagnostic vocabulary).
        let cases: &[(Atom, &str)] = &[
            (Atom::Symbol("foo".into()), "foo"),
            (Atom::Keyword("k".into()), ":k"),
            (Atom::Str("hello".into()), "\"hello\""),
            (Atom::Int(42), "42"),
            (Atom::Int(-7), "-7"),
            (Atom::Float(1.5), "1.5"),
            (Atom::Bool(true), "#t"),
            (Atom::Bool(false), "#f"),
        ];
        for (atom, expected) in cases {
            assert_eq!(
                atom.to_string(),
                *expected,
                "Atom::Display drifted from canonical rendering for {atom:?}"
            );
        }
    }

    #[test]
    fn atom_display_renders_integral_float_with_dot_zero_suffix() {
        // ROUND-TRIP-INVARIANT PIN: `fmt_float`'s `.0`-suffix
        // discipline composes through `Atom::Display` — `Float(1.0)`
        // renders as `"1.0"`, NOT `"1"` (which the reader would
        // re-parse as `Atom::Int(1)`, silently coercing the typed
        // `Float` track into the `Int` track at the Display→read
        // boundary). Direct sibling pin to the existing Display-for-
        // Sexp round-trip tests that exercise the same invariant
        // through the `Sexp::Atom` outer wrap. Lifting the rendering
        // onto the typed `Atom` algebra surfaces a future regression
        // (e.g. an Atom::Display arm that bypasses `fmt_float` and
        // formats `f64` directly) at the atom layer without
        // requiring a Sexp wrap to reproduce.
        assert_eq!(Atom::Float(1.0).to_string(), "1.0");
        assert_eq!(Atom::Float(-42.0).to_string(), "-42.0");
        assert_eq!(Atom::Float(0.99).to_string(), "0.99");
    }

    #[test]
    fn sexp_atom_display_arm_routes_through_atom_display_for_every_variant() {
        // LIFTED-BOUNDARY CONTRACT: pin that `Sexp::Atom(a).to_string()
        // == a.to_string()` for every atomic payload variant. Pre-
        // lift the per-variant body lived inline at the `Sexp::Atom(a)
        // => match a { … }` arm of `Display for Sexp`; post-lift the
        // outer arm delegates to `fmt::Display::fmt(a, f)`. A
        // regression that drifts the outer arm (e.g. wraps the atom
        // rendering in parens, or routes Symbol through a Sexp-
        // specific arm before delegating) surfaces as an inequality
        // here. The cases sweep all six `Atom` variants (Bool unified
        // — both true/false agree under the impl). Sibling posture
        // to the quote-family routing test
        // `sexp_to_json_routes_quote_family_arms_through_as_quote_form_typed_marker`
        // that pins the analogous `Sexp` outer arm routing through
        // a typed algebra projection.
        let cases: &[Atom] = &[
            Atom::Symbol("name".into()),
            Atom::Keyword("kw".into()),
            Atom::Str("body".into()),
            Atom::Int(7),
            Atom::Float(2.5),
            Atom::Float(1.0),
            Atom::Bool(true),
            Atom::Bool(false),
        ];
        for atom in cases {
            let via_sexp = Sexp::Atom(atom.clone()).to_string();
            let via_atom = atom.to_string();
            assert_eq!(
                via_sexp, via_atom,
                "Sexp::Atom Display arm drifted from Atom::Display for {atom:?}"
            );
        }
    }

    #[test]
    fn atom_display_round_trips_through_reader_preserving_typed_identity() {
        // BIDIRECTIONAL TYPED-IDENTITY CONTRACT: render an atom via
        // `Atom::Display`, parse the rendering through
        // `crate::reader::read`, and pin that the parsed value's
        // outer shape is `Sexp::Atom(_)` carrying the SAME variant
        // discriminator as the seed (via `Atom::kind`) AND that the
        // payload round-trips bit-for-bit. This is the typed-exit /
        // typed-entry mirror at the atomic-payload boundary — the
        // load-bearing invariant the `fmt_float` `.0`-suffix
        // discipline already exists to preserve. A regression that
        // drifts ONE side (Display arm OR reader arm) corrupts the
        // round-trip; pin it at the typed boundary directly. Sibling
        // posture to the existing Sexp-layer round-trip tests:
        // `float_display_round_trips_through_reader_into_typed_float`,
        // `quote_prefix_round_trips_through_read_quoted_into_sexp_quote`.
        let cases: &[Atom] = &[
            Atom::Symbol("foo-bar".into()),
            Atom::Keyword("kw".into()),
            Atom::Int(42),
            Atom::Int(-7),
            Atom::Int(0),
            Atom::Float(1.0),
            Atom::Float(1.5),
            Atom::Float(-42.0),
            Atom::Bool(true),
            Atom::Bool(false),
        ];
        for seed in cases {
            let rendered = seed.to_string();
            let mut parsed = crate::reader::read(&rendered)
                .unwrap_or_else(|e| panic!("reader rejected {rendered:?} for {seed:?}: {e}"));
            assert_eq!(
                parsed.len(),
                1,
                "rendered {rendered:?} for {seed:?} re-read as != 1 form"
            );
            let Sexp::Atom(round_tripped) = parsed.remove(0) else {
                panic!("rendered {rendered:?} for {seed:?} re-read as non-Atom");
            };
            assert_eq!(
                round_tripped.kind(),
                seed.kind(),
                "Atom::Display→reader drifted variant for {seed:?} via {rendered:?}"
            );
            assert_eq!(
                round_tripped, *seed,
                "Atom::Display→reader drifted payload for {seed:?} via {rendered:?}"
            );
        }
    }

    #[test]
    fn atom_to_json_projects_each_variant_to_canonical_json_value() {
        // CANONICAL-MAPPING CONTRACT: pin that `Atom::to_json` produces
        // byte-identical `serde_json::Value` outputs for each
        // `AtomKind` variant as the pre-lift inline arms inside
        // `crate::domain::sexp_to_json` did. Sweeps a representative
        // atom of each variant so a regression that drifts ONE arm
        // (e.g. swaps `Symbol`'s mapping to a Number, or drops
        // `Keyword`'s `:` prefix that `json_to_sexp`'s inverse strips
        // — silently breaking every `:values-overlay` payload pinned
        // by the CLAUDE.md bool warning) fails loudly. Sibling-arm
        // sweep to `atom_display_renders_each_variant_to_canonical_form`
        // — both pin the typed-algebra rendering of the atomic
        // payload at its canonical projection. The float case uses
        // `1.5` (finite) here; NaN / ±∞ get their own pin below.
        use serde_json::Value as JValue;
        assert_eq!(
            Atom::Symbol("name".into()).to_json(),
            JValue::String("name".into()),
        );
        assert_eq!(
            Atom::Keyword("parent".into()).to_json(),
            JValue::String(":parent".into()),
        );
        assert_eq!(
            Atom::Str("body".into()).to_json(),
            JValue::String("body".into()),
        );
        assert_eq!(Atom::Int(42).to_json(), JValue::Number(42i64.into()));
        assert_eq!(Atom::Int(-7).to_json(), JValue::Number((-7i64).into()));
        assert_eq!(
            Atom::Float(1.5).to_json(),
            JValue::Number(serde_json::Number::from_f64(1.5).unwrap()),
        );
        assert_eq!(Atom::Bool(true).to_json(), JValue::Bool(true));
        assert_eq!(Atom::Bool(false).to_json(), JValue::Bool(false));
    }

    #[test]
    fn atom_to_json_float_nan_and_infinity_collapse_to_null() {
        // JSON-INEXPRESSIBILITY PIN: JSON has no canonical form for
        // `NaN` / `±∞` — `serde_json::Number::from_f64` returns `None`
        // for those values, and the substrate's pre-lift behavior at
        // `sexp_to_json` mapped them to `JValue::Null` via
        // `unwrap_or(JValue::Null)`. Pin the special-case branch at
        // the typed-algebra boundary directly so a future refactor
        // that bypasses `serde_json::Number::from_f64` (e.g. emits
        // `NaN` as the string `"NaN"`, which the JSON deserializer
        // would silently re-read as a String at the round-trip
        // boundary) surfaces at this test without requiring a Sexp
        // wrap to reproduce. Sibling-shape pin to
        // `atom_display_renders_integral_float_with_dot_zero_suffix`
        // — both pin a non-default branch of the float projection's
        // canonical rendering. The branch IS load-bearing for the
        // `sexp_to_json` → `serde_json::from_value::<T>` bridge the
        // derive-macro fallthrough uses: a downstream `f64` field
        // that the operator wrote `:rate :nan` for collapses to
        // `JValue::Null` HERE rather than at the serde boundary,
        // emitting a clean structural diagnostic instead of a JSON
        // parse error miles downstream.
        use serde_json::Value as JValue;
        assert_eq!(Atom::Float(f64::NAN).to_json(), JValue::Null);
        assert_eq!(Atom::Float(f64::INFINITY).to_json(), JValue::Null);
        assert_eq!(Atom::Float(f64::NEG_INFINITY).to_json(), JValue::Null);
    }

    #[test]
    fn atom_from_lexeme_classifies_each_atom_kind_for_canonical_lexeme() {
        // CANONICAL-CLASSIFICATION CONTRACT: pin that `Atom::from_lexeme`
        // produces byte-identical typed `Atom` outputs for a canonical
        // lexeme of each `AtomKind` variant against the pre-lift
        // `crate::reader::atom_from_str` cascade. Sweeps a representative
        // lexeme of each variant so a regression that drifts ONE arm
        // (e.g. swaps `"#t"` to `Atom::Symbol("#t")` silently breaking
        // every `:values-overlay` payload pinned by the CLAUDE.md bool
        // warning, or strips `":kw"`'s prefix when classifying to
        // `Atom::Symbol` rather than `Atom::Keyword`) fails loudly.
        // Sibling-arm sweep to
        // `atom_display_renders_each_variant_to_canonical_form` and
        // `atom_to_json_projects_each_variant_to_canonical_json_value` —
        // all three pin the typed-algebra at its canonical per-variant
        // projection. This is the typed-ENTRY side of the bidirectional
        // sweep; those are the typed-EXIT sides.
        //
        // `Atom::Str` is intentionally absent — `Atom::from_lexeme`'s
        // typed-entry surface processes BARE reader-token lexemes, and
        // string literals take the reader's `"`-quoted tokenizer branch
        // (a `Token::Str(_)`, NOT a `Token::Atom(_)`). The reader's
        // string round-trip is pinned by `string_escapes` in
        // `crate::reader::tests`.
        assert_eq!(Atom::from_lexeme("foo"), Atom::Symbol("foo".into()));
        assert_eq!(
            Atom::from_lexeme("defpoint"),
            Atom::Symbol("defpoint".into())
        );
        assert_eq!(Atom::from_lexeme("seph.1"), Atom::Symbol("seph.1".into()));
        assert_eq!(Atom::from_lexeme(":parent"), Atom::Keyword("parent".into()));
        assert_eq!(Atom::from_lexeme(":kw"), Atom::Keyword("kw".into()));
        assert_eq!(Atom::from_lexeme("42"), Atom::Int(42));
        assert_eq!(Atom::from_lexeme("-7"), Atom::Int(-7));
        assert_eq!(Atom::from_lexeme("0"), Atom::Int(0));
        assert_eq!(Atom::from_lexeme("1.5"), Atom::Float(1.5));
        assert_eq!(Atom::from_lexeme("-2.5"), Atom::Float(-2.5));
        assert_eq!(Atom::from_lexeme("#t"), Atom::Bool(true));
        assert_eq!(Atom::from_lexeme("#f"), Atom::Bool(false));
    }

    #[test]
    fn atom_from_lexeme_prefers_int_over_float_for_integer_lexeme() {
        // LOAD-BEARING DISPATCH-ORDERING PIN: `Atom::from_lexeme` tries
        // `i64::from_str` BEFORE `f64::from_str` so a bare `"1"`
        // classifies as `Atom::Int(1)`, NOT `Atom::Float(1.0)`. The
        // typed-int-vs-typed-float distinction at the typed-entry
        // boundary is the dual of `fmt_float`'s `.0`-suffix discipline
        // on the typed-exit side — together the two projections form
        // the round-trip identity `from_lexeme(a.to_string()) == a`
        // for both `Int(_)` and `Float(_)` payloads pinned by
        // `atom_from_lexeme_round_trips_with_atom_display_for_every_non_str_variant`
        // below. A regression that reorders the parse-cascade (e.g.
        // tries `f64::from_str` first, or unifies both via
        // `f64::from_str` alone since `f64` parse accepts integer
        // lexemes too) silently demotes every integer authoring slot
        // into the float track at the reader, corrupting every
        // downstream `i64` field's serde round-trip without a
        // structural error to point to.
        assert_eq!(Atom::from_lexeme("1"), Atom::Int(1));
        assert_eq!(Atom::from_lexeme("0"), Atom::Int(0));
        assert_eq!(Atom::from_lexeme("-100"), Atom::Int(-100));
        // The bare-int lexeme MUST NOT classify to `Atom::Float`.
        assert_ne!(Atom::from_lexeme("1"), Atom::Float(1.0));
        // Float lexemes (with explicit `.` or scientific notation)
        // route through the f64 arm — pin the cascade's fallthrough
        // ordering so the int-shortcut doesn't swallow them.
        assert_eq!(Atom::from_lexeme("1.0"), Atom::Float(1.0));
        assert_eq!(Atom::from_lexeme("1.5"), Atom::Float(1.5));
        assert_eq!(Atom::from_lexeme("1e3"), Atom::Float(1e3));
    }

    #[test]
    fn atom_from_lexeme_routes_unknown_lexeme_to_symbol_default() {
        // CLOSED-SET DEFAULT-ARM PIN: every lexeme that didn't match a
        // structural prefix (`"#t"`/`"#f"` for Bool, `":"` prefix for
        // Keyword) or parse as a number (`i64` then `f64`) classifies
        // to `Atom::Symbol(_)` by default — the closed-set fallthrough
        // arm the reader has shipped with from inception. Pin the
        // default-arm projection so a future refactor that adds a new
        // structural prefix (e.g. `"#["` for vector literals, `"#\\x"`
        // for char literals) without updating the default-arm wording
        // cannot silently drift previously-Symbol lexemes into a new
        // bucket — the regression surfaces at this test, which sweeps
        // the structural-prefix non-matches every closed-set extension
        // must continue to classify as Symbol unless the extension
        // explicitly claims them. Sibling-shape pin to
        // `atom_from_lexeme_classifies_each_atom_kind_for_canonical_lexeme`
        // — that pins the structural-prefix MATCHES, this pins the
        // structural-prefix NON-MATCHES.
        //
        // The CLAUDE.md-pinned `true`/`false` round-trip discipline
        // also rides this default arm: bare `true`/`false` re-read as
        // `Atom::Symbol("true")` / `Atom::Symbol("false")` because the
        // Scheme bool spellings are `"#t"`/`"#f"`. The pin guards the
        // `serde_json::Value::Bool` field round-trip every
        // `:values-overlay` payload depends on.
        assert_eq!(Atom::from_lexeme("foo"), Atom::Symbol("foo".into()));
        assert_eq!(
            Atom::from_lexeme("defpoint"),
            Atom::Symbol("defpoint".into())
        );
        // The CLAUDE.md `true`/`false` warning — these lexemes MUST
        // route through the default Symbol arm, NOT through the Bool
        // arm. A regression that adds `"true"`/`"false"` recognition
        // silently flips every `:values-overlay` Bool field to the
        // wrong serde shape.
        assert_eq!(Atom::from_lexeme("true"), Atom::Symbol("true".into()));
        assert_eq!(Atom::from_lexeme("false"), Atom::Symbol("false".into()));
        // Non-structural-prefix shapes — pin a sampling so the
        // default arm continues to absorb every shape the prefix
        // arms haven't claimed.
        assert_eq!(Atom::from_lexeme("seph.1"), Atom::Symbol("seph.1".into()));
        assert_eq!(Atom::from_lexeme("a-b"), Atom::Symbol("a-b".into()));
        assert_eq!(Atom::from_lexeme("+"), Atom::Symbol("+".into()));
    }

    #[test]
    fn atom_from_lexeme_round_trips_with_atom_display_for_every_non_str_variant() {
        // BIDIRECTIONAL TYPED-IDENTITY CONTRACT: render each `Atom`
        // (excluding `Atom::Str` — see below) via `fmt::Display`, parse
        // the rendering through `Atom::from_lexeme`, and pin that the
        // round-trip preserves the typed identity exactly. This is the
        // typed-exit / typed-entry mirror at the atomic-payload
        // boundary AT THE ALGEBRA LEVEL — sibling-shape pin to
        // `atom_display_round_trips_through_reader_preserving_typed_identity`
        // which exercises the same round-trip through the full reader.
        // Lifting the typed-entry surface onto `Atom::from_lexeme`
        // means the round-trip law now lives at the algebra rather
        // than at the reader's free-function boundary — a future
        // tool that wants to round-trip an `Atom` through its
        // canonical lexeme spelling (LSP token-completion, REPL
        // pretty-printer, structural editor) binds to `from_lexeme` +
        // `Display` directly without crossing through the reader's
        // tokenizer.
        //
        // `Atom::Str` is intentionally absent — `Display for Atom`
        // renders `Str(s)` as `"{s:?}"` (debug-quoted, with quote
        // marks around the content). The quoted form is NOT a bare
        // reader-token lexeme: it's a `Token::Str(_)` to the
        // tokenizer, taking a distinct branch. The Str round-trip
        // through the FULL reader is pinned by `string_escapes` in
        // `crate::reader::tests`.
        let cases: &[Atom] = &[
            Atom::Symbol("foo-bar".into()),
            Atom::Symbol("defpoint".into()),
            Atom::Symbol("seph.1".into()),
            Atom::Keyword("parent".into()),
            Atom::Keyword("kw".into()),
            Atom::Int(0),
            Atom::Int(42),
            Atom::Int(-7),
            Atom::Float(1.0),
            Atom::Float(1.5),
            Atom::Float(-42.0),
            Atom::Bool(true),
            Atom::Bool(false),
        ];
        for seed in cases {
            let rendered = seed.to_string();
            let round_tripped = Atom::from_lexeme(&rendered);
            assert_eq!(
                round_tripped.kind(),
                seed.kind(),
                "Atom::from_lexeme∘Display drifted variant for {seed:?} via {rendered:?}"
            );
            assert_eq!(
                round_tripped, *seed,
                "Atom::from_lexeme∘Display drifted payload for {seed:?} via {rendered:?}"
            );
        }
    }

    // ── Atom::as_X soft-projection family + Sexp::as_atom structural lift ──
    //
    // The six per-variant soft-projection methods on the typed `Atom` algebra
    // (`as_symbol` / `as_keyword` / `as_string` / `as_int` / `as_float` /
    // `as_bool`) lift the inline `Self::Atom(Atom::X(s)) => Some(s)` arms
    // that previously lived at the six `Sexp::as_X` consumer sites onto ONE
    // method per closed-set arm. The `Sexp::as_atom` structural lift gives
    // the consumer family a uniform two-step composition `as_atom().and_then
    // (Atom::as_X)`. The tests below pin:
    //
    //   (1) per-variant typed projection — `Atom::as_X` returns `Some(payload)`
    //       iff the variant matches AND `None` for every other closed-set arm
    //       (path-uniformity over `AtomKind::ALL`);
    //   (2) the `Sexp::as_atom` projection — `Some(&Atom)` iff `Sexp::Atom(_)`
    //       AND `None` for the structural shapes (`Nil` / `List` /
    //       `Quote` / `Quasiquote` / `Unquote` / `UnquoteSplice`);
    //   (3) lifted-boundary composition — `Sexp::as_<X>(s) == s.as_atom()
    //       .and_then(Atom::as_<X>)` for every atomic variant, AND the
    //       `Sexp::as_float` widening specialization (`Atom::Int(n)` →
    //       `Some(n as f64)`) lives at the consumer layer, NOT the algebra
    //       layer (per the typed-identity discipline pinned at
    //       `Atom::as_int`'s docstring).

    #[test]
    fn atom_as_symbol_returns_payload_iff_symbol_variant() {
        // PER-VARIANT PROJECTION CONTRACT: `Atom::as_symbol` projects
        // `Atom::Symbol(s)` to `Some(&s)` and every other `AtomKind`
        // variant to `None`. Sweeps `AtomKind::ALL` for the path-
        // uniformity guard — catches a regression that mis-routes ONE
        // arm (e.g. accepts `Atom::Keyword(s)` thinking it's "also a
        // symbol-like identifier", or rejects `Atom::Symbol("foo")` if
        // a future closed-set sweep accidentally narrows the projection
        // by an `s.is_empty()` filter).
        assert_eq!(Atom::Symbol("foo".into()).as_symbol(), Some("foo"));
        assert_eq!(Atom::Symbol("seph.1".into()).as_symbol(), Some("seph.1"));
        assert_eq!(Atom::Symbol(String::new()).as_symbol(), Some(""));
        for kind in AtomKind::ALL {
            if kind == AtomKind::Symbol {
                continue;
            }
            let probe: Atom = match kind {
                AtomKind::Symbol => unreachable!(),
                AtomKind::Keyword => Atom::Keyword("kw".into()),
                AtomKind::Str => Atom::Str("body".into()),
                AtomKind::Int => Atom::Int(42),
                AtomKind::Float => Atom::Float(1.5),
                AtomKind::Bool => Atom::Bool(true),
            };
            assert_eq!(
                probe.as_symbol(),
                None,
                "Atom::as_symbol must reject non-Symbol variant {kind:?}",
            );
        }
    }

    #[test]
    fn atom_as_keyword_returns_payload_iff_keyword_variant() {
        // PER-VARIANT PROJECTION CONTRACT: `Atom::as_keyword` projects
        // `Atom::Keyword(s)` to `Some(&s)` and every other `AtomKind`
        // variant to `None`. The returned `&str` is the BARE identifier
        // (the `:` prefix was already stripped at the typed-ENTRY
        // classifier boundary, `Atom::from_lexeme`); this projection
        // does not re-add or re-strip the prefix — pinned by the empty
        // probe to catch a regression that accidentally trims a leading
        // char.
        assert_eq!(Atom::Keyword("parent".into()).as_keyword(), Some("parent"));
        assert_eq!(Atom::Keyword(String::new()).as_keyword(), Some(""));
        for kind in AtomKind::ALL {
            if kind == AtomKind::Keyword {
                continue;
            }
            let probe: Atom = match kind {
                AtomKind::Symbol => Atom::Symbol("foo".into()),
                AtomKind::Keyword => unreachable!(),
                AtomKind::Str => Atom::Str("body".into()),
                AtomKind::Int => Atom::Int(42),
                AtomKind::Float => Atom::Float(1.5),
                AtomKind::Bool => Atom::Bool(true),
            };
            assert_eq!(
                probe.as_keyword(),
                None,
                "Atom::as_keyword must reject non-Keyword variant {kind:?}",
            );
        }
    }

    #[test]
    fn atom_as_string_returns_payload_iff_str_variant() {
        // PER-VARIANT PROJECTION CONTRACT: `Atom::as_string` projects
        // `Atom::Str(s)` to `Some(&s)` and every other `AtomKind`
        // variant (including `Symbol` and `Keyword`, which also carry
        // `String` payloads) to `None`. The closed-set discriminator is
        // load-bearing: a `Symbol("foo")` MUST NOT route through this
        // projection — a regression that conflates the three string-
        // carrying variants would silently re-classify operator-position
        // symbols as string-typed kwarg values at every `extract_string`
        // boundary.
        assert_eq!(Atom::Str("body".into()).as_string(), Some("body"));
        assert_eq!(
            Atom::Str("with\nnewline".into()).as_string(),
            Some("with\nnewline"),
        );
        assert_eq!(Atom::Str(String::new()).as_string(), Some(""));
        assert_eq!(
            Atom::Symbol("looks-like-a-string".into()).as_string(),
            None,
            "Atom::as_string MUST NOT conflate Symbol with Str — load-bearing typed-identity",
        );
        assert_eq!(
            Atom::Keyword("looks-like-a-string".into()).as_string(),
            None,
            "Atom::as_string MUST NOT conflate Keyword with Str — load-bearing typed-identity",
        );
        for kind in [AtomKind::Int, AtomKind::Float, AtomKind::Bool] {
            let probe: Atom = match kind {
                AtomKind::Int => Atom::Int(42),
                AtomKind::Float => Atom::Float(1.5),
                AtomKind::Bool => Atom::Bool(true),
                _ => unreachable!(),
            };
            assert_eq!(
                probe.as_string(),
                None,
                "Atom::as_string must reject non-Str variant {kind:?}",
            );
        }
    }

    #[test]
    fn atom_as_int_returns_payload_iff_int_variant_strict_no_float_widening() {
        // PER-VARIANT PROJECTION CONTRACT (STRICT): `Atom::as_int`
        // projects `Atom::Int(n)` to `Some(n)` and every other variant
        // to `None`. STRICT typed identity: `Atom::Float(1.0)` does
        // NOT project through (stays `None`) — the typed-identity
        // distinction `Int(1)` vs `Float(1.0)` (load-bearing at the
        // `Atom::from_lexeme` ⇄ `Atom::Display` round-trip boundary, dual of
        // `fmt_float`'s `.0`-suffix discipline) is preserved at the
        // algebra layer. The widening face lives at the
        // `Sexp::as_float` consumer (which accepts both `Float` AND
        // `Int`); the strict typed identity at the `Atom` algebra is
        // load-bearing.
        assert_eq!(Atom::Int(42).as_int(), Some(42));
        assert_eq!(Atom::Int(-7).as_int(), Some(-7));
        assert_eq!(Atom::Int(0).as_int(), Some(0));
        assert_eq!(
            Atom::Float(1.0).as_int(),
            None,
            "Atom::as_int MUST be strict — Float(1.0) is NOT Int(1) at the algebra layer",
        );
        for kind in [
            AtomKind::Symbol,
            AtomKind::Keyword,
            AtomKind::Str,
            AtomKind::Float,
            AtomKind::Bool,
        ] {
            let probe: Atom = match kind {
                AtomKind::Symbol => Atom::Symbol("foo".into()),
                AtomKind::Keyword => Atom::Keyword("kw".into()),
                AtomKind::Str => Atom::Str("body".into()),
                AtomKind::Float => Atom::Float(1.5),
                AtomKind::Bool => Atom::Bool(true),
                _ => unreachable!(),
            };
            assert_eq!(
                probe.as_int(),
                None,
                "Atom::as_int must reject non-Int variant {kind:?}",
            );
        }
    }

    #[test]
    fn atom_as_float_returns_payload_iff_float_variant_strict_no_int_widening() {
        // PER-VARIANT PROJECTION CONTRACT (STRICT): `Atom::as_float`
        // projects `Atom::Float(n)` to `Some(n)` and every other
        // variant to `None`. STRICT typed identity: `Atom::Int(1)`
        // does NOT project through (stays `None`) — see
        // `atom_as_int_returns_payload_iff_int_variant_strict_no_float_widening`
        // for the symmetric discipline. The widening face
        // (`Atom::Int(n) → Some(n as f64)`) lives at the `Sexp::as_float`
        // consumer layer, NOT the algebra layer.
        assert_eq!(Atom::Float(1.5).as_float(), Some(1.5));
        assert_eq!(Atom::Float(1.0).as_float(), Some(1.0));
        assert_eq!(Atom::Float(-42.0).as_float(), Some(-42.0));
        assert_eq!(
            Atom::Int(1).as_float(),
            None,
            "Atom::as_float MUST be strict — Int(1) is NOT Float(1.0) at the algebra layer",
        );
        for kind in [
            AtomKind::Symbol,
            AtomKind::Keyword,
            AtomKind::Str,
            AtomKind::Int,
            AtomKind::Bool,
        ] {
            let probe: Atom = match kind {
                AtomKind::Symbol => Atom::Symbol("foo".into()),
                AtomKind::Keyword => Atom::Keyword("kw".into()),
                AtomKind::Str => Atom::Str("body".into()),
                AtomKind::Int => Atom::Int(42),
                AtomKind::Bool => Atom::Bool(true),
                _ => unreachable!(),
            };
            assert_eq!(
                probe.as_float(),
                None,
                "Atom::as_float must reject non-Float variant {kind:?}",
            );
        }
    }

    #[test]
    fn atom_as_bool_returns_payload_iff_bool_variant() {
        // PER-VARIANT PROJECTION CONTRACT: `Atom::as_bool` projects
        // `Atom::Bool(b)` to `Some(b)` and every other variant to
        // `None`. Both spellings (`true` / `false`) project through
        // the SAME projection — the variant identity (`Bool`) is what
        // routes; the inner payload (`true` / `false`) is the
        // projected value. CLAUDE.md "Lisp bools": at the reader
        // boundary the typed-entry classifier `Atom::from_lexeme`
        // routes `"#t"` / `"#f"` to `Atom::Bool(_)` and bare
        // `"true"` / `"false"` to `Atom::Symbol(_)`; this projection
        // does NOT re-classify the symbol-spelled bools — they STAY
        // symbols. The negative test (`Atom::Symbol("true")` rejects)
        // pins the discriminator discipline.
        assert_eq!(Atom::Bool(true).as_bool(), Some(true));
        assert_eq!(Atom::Bool(false).as_bool(), Some(false));
        assert_eq!(
            Atom::Symbol("true".into()).as_bool(),
            None,
            "Atom::as_bool MUST reject Symbol(\"true\") — CLAUDE.md typed-identity discipline",
        );
        assert_eq!(
            Atom::Symbol("false".into()).as_bool(),
            None,
            "Atom::as_bool MUST reject Symbol(\"false\") — CLAUDE.md typed-identity discipline",
        );
        for kind in [
            AtomKind::Symbol,
            AtomKind::Keyword,
            AtomKind::Str,
            AtomKind::Int,
            AtomKind::Float,
        ] {
            let probe: Atom = match kind {
                AtomKind::Symbol => Atom::Symbol("foo".into()),
                AtomKind::Keyword => Atom::Keyword("kw".into()),
                AtomKind::Str => Atom::Str("body".into()),
                AtomKind::Int => Atom::Int(42),
                AtomKind::Float => Atom::Float(1.5),
                _ => unreachable!(),
            };
            assert_eq!(
                probe.as_bool(),
                None,
                "Atom::as_bool must reject non-Bool variant {kind:?}",
            );
        }
    }

    #[test]
    fn atom_as_symbol_or_string_returns_payload_iff_symbol_or_str_variant() {
        // UNION-PROJECTION CONTRACT: `Atom::as_symbol_or_string` projects
        // BOTH `Atom::Symbol(s)` AND `Atom::Str(s)` to `Some(s)` and every
        // other atomic kind (`Keyword`, `Int`, `Float`, `Bool`) to `None`.
        // The disjunctive composition `as_symbol().or_else(||
        // as_string())` lives at ONE typed-algebra projection on the
        // closed-set `Atom` algebra; pre-lift the composition lived at
        // `Sexp::as_symbol_or_string`'s consumer body and traversed
        // `Sexp::as_atom` TWICE (once per per-variant projection),
        // post-lift it traverses `Sexp::as_atom` ONCE through the
        // algebra-level union projection. Pin the algebra-level contract
        // sweep so a regression that drifts ONE union arm (e.g. drops the
        // `Str` arm, accidentally widens to accept `Keyword`) surfaces
        // structurally.
        assert_eq!(
            Atom::Symbol("my-name".into()).as_symbol_or_string(),
            Some("my-name"),
            "Atom::as_symbol_or_string must accept Atom::Symbol",
        );
        assert_eq!(
            Atom::Str("my-name".into()).as_symbol_or_string(),
            Some("my-name"),
            "Atom::as_symbol_or_string must accept Atom::Str",
        );
        // Empty payloads project through too — the union projection
        // is keyed on variant identity, not payload contents.
        assert_eq!(
            Atom::Symbol(String::new()).as_symbol_or_string(),
            Some(""),
            "Atom::as_symbol_or_string must accept empty Symbol payload",
        );
        assert_eq!(
            Atom::Str(String::new()).as_symbol_or_string(),
            Some(""),
            "Atom::as_symbol_or_string must accept empty Str payload",
        );
        // Negative sweep: the four non-Symbol-non-Str variants reject.
        for kind in [
            AtomKind::Keyword,
            AtomKind::Int,
            AtomKind::Float,
            AtomKind::Bool,
        ] {
            let probe: Atom = match kind {
                AtomKind::Keyword => Atom::Keyword("kw".into()),
                AtomKind::Int => Atom::Int(42),
                AtomKind::Float => Atom::Float(1.5),
                AtomKind::Bool => Atom::Bool(true),
                _ => unreachable!(),
            };
            assert_eq!(
                probe.as_symbol_or_string(),
                None,
                "Atom::as_symbol_or_string must reject non-Symbol-non-Str variant {kind:?}",
            );
        }
    }

    #[test]
    fn atom_as_symbol_or_string_borrow_ptr_eq_payload() {
        // BORROW-LIFETIME CONTRACT: the yielded `&str` borrows the inner
        // `String` payload's `&str` view verbatim — no copy, no
        // allocation, no `to_string()` round-trip. Pin via `ptr::eq` on
        // both projection sides (Symbol arm AND Str arm) so a regression
        // that re-inlines the union as `match self { Symbol(s) =>
        // Some(s.clone().as_str()), … }` (a `String::clone` reborrow that
        // changes the byte-identity) surfaces structurally. Same posture
        // as `as_call_to_args_borrow_is_same_pointer_as_as_call_tail` on
        // the call-form algebra.
        let sym = Atom::Symbol("my-name".into());
        let projected = sym.as_symbol_or_string().expect("Symbol arm projects");
        match &sym {
            Atom::Symbol(s) => assert!(
                std::ptr::eq(projected.as_ptr(), s.as_ptr()),
                "Atom::as_symbol_or_string must borrow Atom::Symbol payload verbatim",
            ),
            _ => unreachable!(),
        }
        let str_atom = Atom::Str("my-name".into());
        let projected_str = str_atom.as_symbol_or_string().expect("Str arm projects");
        match &str_atom {
            Atom::Str(s) => assert!(
                std::ptr::eq(projected_str.as_ptr(), s.as_ptr()),
                "Atom::as_symbol_or_string must borrow Atom::Str payload verbatim",
            ),
            _ => unreachable!(),
        }
    }

    #[test]
    fn atom_as_symbol_or_string_is_the_disjunction_of_as_symbol_and_as_string() {
        // COMPOSITION LAW: pin that the union projection's value AGREES
        // byte-for-byte with the explicit disjunctive composition
        // `as_symbol().or_else(|| as_string())` across every atom kind.
        // A regression that drifts the union from its disjunctive
        // composition (e.g. swaps the `or_else` order so an
        // `Atom::Symbol` somehow routes through the `Str` arm first, or
        // adds a phantom arm that accepts `Keyword` payloads) surfaces
        // here. Same posture as `is_kwargs_list` composing through
        // `as_list ∘ atom_as_keyword`.
        for atom in [
            Atom::Symbol("foo".into()),
            Atom::Keyword("kw".into()),
            Atom::Str("body".into()),
            Atom::Int(42),
            Atom::Float(1.5),
            Atom::Bool(true),
            Atom::Bool(false),
            Atom::Symbol(String::new()),
            Atom::Str(String::new()),
        ] {
            let by_hand = atom.as_symbol().or_else(|| atom.as_string());
            assert_eq!(
                atom.as_symbol_or_string(),
                by_hand,
                "Atom::as_symbol_or_string drifted from as_symbol().or_else(|| as_string()) for {atom:?}",
            );
        }
    }

    #[test]
    fn sexp_as_symbol_or_string_routes_through_atom_as_symbol_or_string_via_as_atom_composition() {
        // CONSUMER-LAYER COMPOSITION LAW: pin that `Sexp::as_symbol_or_string`
        // routes through the structural lift `Sexp::as_atom` + the
        // algebra-level `Atom::as_symbol_or_string` union projection —
        // a regression that re-inlines the pre-lift body
        // `self.as_symbol().or_else(|| self.as_string())` (TWO
        // `Sexp::as_atom` traversals) at the `Sexp` consumer layer
        // becomes detectable here. Sweeps every reachable outer shape so
        // the closed-form composition is pinned across Nil + every Atom
        // variant + every quote-family wrapper + List + the Sexp::Atom
        // arms a regression could route to.
        let cases = [
            Sexp::Nil,
            Sexp::symbol("foo"),
            Sexp::symbol(""),
            Sexp::string("body"),
            Sexp::string(""),
            Sexp::keyword("kw"),
            Sexp::int(7),
            Sexp::float(2.5),
            Sexp::boolean(true),
            Sexp::Quote(Box::new(Sexp::symbol("x"))),
            Sexp::Quasiquote(Box::new(Sexp::symbol("x"))),
            Sexp::Unquote(Box::new(Sexp::symbol("x"))),
            Sexp::UnquoteSplice(Box::new(Sexp::symbol("x"))),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::List(vec![]),
        ];
        for s in &cases {
            let by_composition = s.as_atom().and_then(Atom::as_symbol_or_string);
            assert_eq!(
                s.as_symbol_or_string(),
                by_composition,
                "Sexp::as_symbol_or_string drifted from as_atom().and_then(Atom::as_symbol_or_string) for {s}",
            );
        }
    }

    #[test]
    fn sexp_as_symbol_or_string_yields_none_for_non_atom_outer_shapes() {
        // OUTER-SHAPE NEGATIVE SWEEP: pin that every non-Atom outer
        // shape (`Nil`, `List`, every quote-family wrapper) projects to
        // `None` — the structural-lift `Sexp::as_atom` rejects them at
        // the outer match before the union projection even runs. Pins
        // the soft-projection face: the named-form NAME gate
        // (`crate::compile::split_name_slot`'s `as_symbol_or_string`
        // consumer at compile.rs:671) sees `None` for these shapes and
        // emits `NamedFormNonSymbolName` with the projected `SexpShape`
        // — the lift preserves the same rejection arm boundary.
        for outer in [
            Sexp::Nil,
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::List(vec![]),
            Sexp::Quote(Box::new(Sexp::symbol("x"))),
            Sexp::Quasiquote(Box::new(Sexp::symbol("x"))),
            Sexp::Unquote(Box::new(Sexp::symbol("x"))),
            Sexp::UnquoteSplice(Box::new(Sexp::symbol("x"))),
        ] {
            assert_eq!(
                outer.as_symbol_or_string(),
                None,
                "Sexp::as_symbol_or_string must reject non-Atom outer shape {outer:?}",
            );
        }
    }

    #[test]
    fn sexp_as_symbol_or_string_borrow_ptr_eq_atom_payload() {
        // BORROW-LIFETIME CONTRACT: the yielded `&str` borrows the inner
        // `Atom::Symbol` / `Atom::Str` payload verbatim — no copy, no
        // allocation, same lifetime as the outer `&Sexp`. Pin via
        // `ptr::eq` on both projection sides so a regression that
        // re-inlines the union as a `String`-allocating reborrow (e.g.
        // `.map(|s| s.to_owned())` somewhere along the chain) surfaces
        // structurally. Sibling pin to
        // `atom_as_symbol_or_string_borrow_ptr_eq_payload` at the outer
        // (`&Sexp`) layer rather than the inner (`&Atom`) layer.
        let sym_sexp = Sexp::symbol("my-name");
        let projected = sym_sexp.as_symbol_or_string().expect("Symbol arm projects");
        match &sym_sexp {
            Sexp::Atom(Atom::Symbol(s)) => assert!(
                std::ptr::eq(projected.as_ptr(), s.as_ptr()),
                "Sexp::as_symbol_or_string must borrow Atom::Symbol payload verbatim",
            ),
            _ => unreachable!(),
        }
        let str_sexp = Sexp::string("my-name");
        let projected_str = str_sexp.as_symbol_or_string().expect("Str arm projects");
        match &str_sexp {
            Sexp::Atom(Atom::Str(s)) => assert!(
                std::ptr::eq(projected_str.as_ptr(), s.as_ptr()),
                "Sexp::as_symbol_or_string must borrow Atom::Str payload verbatim",
            ),
            _ => unreachable!(),
        }
    }

    #[test]
    fn sexp_as_atom_projects_inner_atom_iff_outer_is_atom_variant() {
        // STRUCTURAL-LIFT CONTRACT: `Sexp::as_atom` projects
        // `Sexp::Atom(a)` to `Some(&a)` and every other outer shape
        // (`Nil` / `List` / `Quote` / `Quasiquote` / `Unquote` /
        // `UnquoteSplice`) to `None`. Sweeps each outer shape so a
        // regression that mis-routes ONE arm (e.g. accepts the
        // singleton list `(a)` thinking the inner counts as the
        // "wrapped atom", or rejects an `Atom` whose payload is empty)
        // fails loudly. The `&Atom` borrow is rooted at the outer
        // `&Sexp` — the projection does not clone, allocate, or take
        // ownership.
        let atom = Atom::Symbol("foo".into());
        let sexp = Sexp::Atom(atom.clone());
        assert_eq!(sexp.as_atom(), Some(&atom));

        for outer in [
            Sexp::Nil,
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::List(vec![]),
            Sexp::Quote(Box::new(Sexp::symbol("x"))),
            Sexp::Quasiquote(Box::new(Sexp::symbol("x"))),
            Sexp::Unquote(Box::new(Sexp::symbol("x"))),
            Sexp::UnquoteSplice(Box::new(Sexp::symbol("x"))),
        ] {
            assert_eq!(
                outer.as_atom(),
                None,
                "Sexp::as_atom must reject non-Atom outer shape {outer:?}",
            );
        }
    }

    #[test]
    fn sexp_shape_method_projects_each_outer_arm_to_canonical_sexp_shape() {
        // CANONICAL-MAPPING CONTRACT: pin that `Sexp::shape()` produces
        // byte-identical `SexpShape` markers for each outer-arm of the
        // closed `Sexp` algebra. Sweeps every reachable outer shape
        // (`Nil`, every `AtomKind` payload, `List`, every `QuoteForm`
        // wrapper) so a regression that drifts ONE arm (e.g. routes the
        // `Atom::Keyword` arm through `Atom::kind().sexp_shape()` to the
        // wrong `SexpShape` variant, or drops the `expect_quote_form`
        // projection's marker for a quote-family wrapper) fails loudly.
        // Sibling-arm sweep to
        // `quote_form_sexp_shape_pins_canonical_shape_identity_for_every_variant`
        // (the four quote-family arms in isolation) AND
        // `atom_kind_sexp_shape_pins_canonical_atom_payload_shape_for_every_variant`
        // (the six atomic-payload arms in isolation) — this test pins
        // the OUTER projection that COMPOSES both peer algebras + the
        // `Nil` / `List` arms into ONE typed method on the `Sexp`
        // algebra.
        use crate::error::SexpShape;
        assert_eq!(Sexp::Nil.shape(), SexpShape::Nil);
        assert_eq!(Sexp::symbol("foo").shape(), SexpShape::Symbol);
        assert_eq!(Sexp::keyword("k").shape(), SexpShape::Keyword);
        assert_eq!(Sexp::string("s").shape(), SexpShape::String);
        assert_eq!(Sexp::int(7).shape(), SexpShape::Int);
        assert_eq!(Sexp::float(7.5).shape(), SexpShape::Float);
        assert_eq!(Sexp::boolean(true).shape(), SexpShape::Bool);
        assert_eq!(Sexp::List(vec![]).shape(), SexpShape::List);
        assert_eq!(
            Sexp::List(vec![Sexp::symbol("op"), Sexp::int(1)]).shape(),
            SexpShape::List,
            "non-empty list must project to SexpShape::List — payload count is irrelevant",
        );
        assert_eq!(Sexp::Quote(Box::new(Sexp::Nil)).shape(), SexpShape::Quote);
        assert_eq!(
            Sexp::Quasiquote(Box::new(Sexp::Nil)).shape(),
            SexpShape::Quasiquote
        );
        assert_eq!(
            Sexp::Unquote(Box::new(Sexp::Nil)).shape(),
            SexpShape::Unquote
        );
        assert_eq!(
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)).shape(),
            SexpShape::UnquoteSplice
        );
    }

    #[test]
    fn sexp_shape_method_agrees_with_domain_sexp_shape_for_every_outer_shape() {
        // LIFTED-BOUNDARY CONTRACT: pin that the inherent
        // `Sexp::shape()` method agrees with the free-function
        // delegate `crate::domain::sexp_shape` for every reachable
        // outer shape. Pre-lift the dispatcher lived as a free
        // function in `domain.rs`; post-lift the canonical site is
        // the inherent method on the `Sexp` algebra and the free
        // function is a one-line delegate. Pin that the delegation
        // stays byte-for-byte equivalent across every outer arm so a
        // regression where the free function drifts from the inherent
        // method (or vice versa) surfaces here immediately. Catches
        // a future "consolidation" that removes the free function
        // without updating the method, or vice versa.
        let samples = [
            Sexp::Nil,
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::int(-1),
            Sexp::float(7.5),
            Sexp::float(0.0),
            Sexp::boolean(true),
            Sexp::boolean(false),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("op"), Sexp::int(1), Sexp::int(2)]),
            Sexp::Quote(Box::new(Sexp::symbol("payload"))),
            Sexp::Quasiquote(Box::new(Sexp::List(vec![Sexp::symbol("foo")]))),
            Sexp::Unquote(Box::new(Sexp::symbol("x"))),
            Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))),
        ];
        for s in &samples {
            let via_method = s.shape();
            let via_delegate = crate::domain::sexp_shape(s);
            assert_eq!(
                via_method, via_delegate,
                "Sexp::shape and domain::sexp_shape drifted at {s:?}",
            );
        }
    }

    #[test]
    fn sexp_shape_method_routes_atom_arm_through_atom_kind_sexp_shape_projection() {
        // PATH-UNIFORMITY CONTRACT (atomic axis): the lifted
        // `Sexp::shape()` routes its Atom arm through
        // `Atom::kind().sexp_shape()` — the typed closed-set projection
        // on the `AtomKind` algebra. Pin that the composition agrees
        // bit-for-bit with the direct `Sexp::shape()` projection across
        // every atomic kind variant. A regression in EITHER projection
        // direction (an `Atom::kind` arm that swaps markers, or an
        // `AtomKind::sexp_shape` arm that drifts its `SexpShape` mapping)
        // surfaces here immediately. Sibling shape to
        // `sexp_shape_method_routes_quote_family_arms_through_quote_form_sexp_shape_projection`
        // for the quote-family axis.
        for kind in AtomKind::ALL {
            let atom = match kind {
                AtomKind::Symbol => Atom::Symbol("name".into()),
                AtomKind::Keyword => Atom::Keyword("parent".into()),
                AtomKind::Str => Atom::Str("body".into()),
                AtomKind::Int => Atom::Int(42),
                AtomKind::Float => Atom::Float(1.5),
                AtomKind::Bool => Atom::Bool(true),
            };
            let via_outer = Sexp::Atom(atom.clone()).shape();
            let via_composed = atom.kind().sexp_shape();
            assert_eq!(
                via_outer, via_composed,
                "Sexp::shape's Atom arm drifted from Atom::kind().sexp_shape() at {kind:?}",
            );
        }
    }

    #[test]
    fn sexp_shape_method_routes_quote_family_arms_through_quote_form_sexp_shape_projection() {
        // PATH-UNIFORMITY CONTRACT (quote-family axis): the lifted
        // `Sexp::shape()` routes its four quote-family arms through
        // `as_quote_form() + QuoteForm::sexp_shape()`. Pin that the
        // composition agrees bit-for-bit with the direct `Sexp::shape()`
        // projection across every quote-family wrapper variant. A
        // regression in EITHER projection direction (an `as_quote_form`
        // arm that swaps markers, or a `QuoteForm::sexp_shape` arm that
        // drifts its `SexpShape` mapping) surfaces here immediately.
        // Mirrors the atomic-axis test
        // `sexp_shape_method_routes_atom_arm_through_atom_kind_sexp_shape_projection`.
        let samples = [
            (
                Sexp::Quote(Box::new(Sexp::symbol("payload"))),
                QuoteForm::Quote,
            ),
            (
                Sexp::Quasiquote(Box::new(Sexp::symbol("payload"))),
                QuoteForm::Quasiquote,
            ),
            (
                Sexp::Unquote(Box::new(Sexp::symbol("payload"))),
                QuoteForm::Unquote,
            ),
            (
                Sexp::UnquoteSplice(Box::new(Sexp::symbol("payload"))),
                QuoteForm::UnquoteSplice,
            ),
        ];
        for (sexp, expected_qf) in &samples {
            let via_outer = sexp.shape();
            let (qf, _) = sexp
                .as_quote_form()
                .expect("quote-family sample must project through as_quote_form");
            assert_eq!(
                qf, *expected_qf,
                "as_quote_form drifted typed marker at {sexp:?}"
            );
            let via_composed = qf.sexp_shape();
            assert_eq!(
                via_outer, via_composed,
                "Sexp::shape drifted from as_quote_form + QuoteForm::sexp_shape at {sexp:?}"
            );
        }
    }

    #[test]
    fn sexp_shape_method_routes_structural_arms_through_structural_kind_sexp_shape_projection() {
        // PATH-UNIFORMITY CONTRACT (structural-residual axis): the
        // lifted `Sexp::shape()` routes its two structural-residual
        // arms (Nil, List) through `StructuralKind::sexp_shape()`. Pin
        // that the composition agrees bit-for-bit with the direct
        // `Sexp::shape()` projection across the two structural-residual
        // variants. A regression that drifts EITHER projection direction
        // (a `Sexp::shape` arm that inlines `SexpShape::Nil` /
        // `SexpShape::List` back as a raw literal, or a
        // `StructuralKind::sexp_shape` arm that drifts its `SexpShape`
        // mapping) surfaces here immediately. Sibling-shape pin to the
        // atomic-axis routing test
        // `sexp_shape_method_routes_atom_arm_through_atom_kind_sexp_shape_projection`
        // and the quote-family-axis routing test
        // `sexp_shape_method_routes_quote_family_arms_through_quote_form_sexp_shape_projection`
        // — together the three tests pin ALL THREE closed-set
        // carving-marker `sexp_shape` compositions the lifted
        // `Sexp::shape()` body owns.
        let samples = [
            (Sexp::Nil, StructuralKind::Nil),
            (Sexp::List(vec![]), StructuralKind::List),
            (Sexp::List(vec![Sexp::symbol("a")]), StructuralKind::List),
        ];
        for (sexp, expected_sk) in &samples {
            let via_outer = sexp.shape();
            let sk = sexp
                .as_structural_kind()
                .expect("structural-residual sample must project through as_structural_kind");
            assert_eq!(
                sk, *expected_sk,
                "as_structural_kind drifted typed marker at {sexp:?}"
            );
            let via_composed = sk.sexp_shape();
            assert_eq!(
                via_outer, via_composed,
                "Sexp::shape drifted from as_structural_kind + StructuralKind::sexp_shape at {sexp:?}"
            );
        }
    }

    #[test]
    fn sexp_as_structural_kind_projects_nil_and_list_to_canonical_structural_kind() {
        // PER-ARM CONTRACT: pin that `Sexp::as_structural_kind()`
        // projects `Sexp::Nil` to `Some(StructuralKind::Nil)` and
        // `Sexp::List(_)` to `Some(StructuralKind::List)` — the two
        // structural-residual arms of the `Sexp` algebra. A regression
        // that swaps the two arms (routes `Nil` to `Some(List)` or
        // vice versa), returns `None` for either, or projects to a
        // wrong `StructuralKind` variant surfaces here immediately.
        // The List arm is exercised with an empty AND a non-empty
        // items slice so a body that gates on `items.is_empty()`
        // (rather than the outer arm) fails loudly.
        assert_eq!(Sexp::Nil.as_structural_kind(), Some(StructuralKind::Nil));
        assert_eq!(
            Sexp::List(vec![]).as_structural_kind(),
            Some(StructuralKind::List)
        );
        assert_eq!(
            Sexp::List(vec![Sexp::symbol("a")]).as_structural_kind(),
            Some(StructuralKind::List)
        );
        assert_eq!(
            Sexp::List(vec![Sexp::int(1), Sexp::int(2), Sexp::int(3)]).as_structural_kind(),
            Some(StructuralKind::List)
        );
    }

    #[test]
    fn sexp_as_structural_kind_rejects_non_structural_outer_shapes() {
        // KERNEL CONTRACT: pin that `Sexp::as_structural_kind()`
        // returns `None` for every non-structural outer shape — every
        // `Sexp::Atom` variant (the atomic-payload carving) AND every
        // quote-family wrapper (the quote-family carving). Sweeps
        // every non-residual arm so a regression that accepts an atom
        // (e.g. routes `Sexp::Atom(_)` to `Some(List)` because the
        // outer arm is misread as a "container" of an atomic payload)
        // or a quote-family wrapper (e.g. routes `Sexp::Quote(_)`
        // through `_ => Some(_)` because the residual match falls
        // through) fails loudly. Sibling-cohort sweep to
        // `sexp_as_atom_projects_inner_atom_iff_outer_is_atom_variant`
        // — that test pins the atomic-projection kernel, this one
        // pins the structural-residual kernel.
        for outer in [
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::Quote(Box::new(Sexp::symbol("x"))),
            Sexp::Quasiquote(Box::new(Sexp::symbol("x"))),
            Sexp::Unquote(Box::new(Sexp::symbol("x"))),
            Sexp::UnquoteSplice(Box::new(Sexp::symbol("x"))),
        ] {
            assert_eq!(
                outer.as_structural_kind(),
                None,
                "Sexp::as_structural_kind must reject non-structural outer shape {outer:?}",
            );
        }
    }

    #[test]
    fn sexp_as_structural_kind_agrees_with_shape_as_structural_kind_for_every_variant() {
        // COMPOSITION-LAW CONTRACT: `s.as_structural_kind() ==
        // s.shape().as_structural_kind()` for every reachable Sexp
        // outer shape. The value-level projection and the shape-level
        // projection MUST agree bit-for-bit — the substrate's
        // (Sexp value, StructuralKind marker) pairing binds at TWO
        // typed methods (one on `Sexp`, one on `SexpShape`) that must
        // stay in lockstep. Sweeps every outer shape (residual + atom
        // + quote-family) so a drift on ANY arm surfaces immediately.
        // Sibling-shape pin to the (Sexp → SexpShape → label) path-
        // uniformity test
        // `sexp_shape_method_label_composes_with_sexp_type_name_for_every_outer_shape`
        // — where that test pins the label-projection composition,
        // this one pins the structural-carving-marker projection
        // composition.
        let samples = [
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ];
        for s in &samples {
            assert_eq!(
                s.as_structural_kind(),
                s.shape().as_structural_kind(),
                "Sexp::as_structural_kind and Sexp::shape().as_structural_kind must agree at {s:?}",
            );
        }
    }

    #[test]
    fn sexp_as_structural_kind_partitions_outer_shapes_jointly_with_as_atom_and_as_quote_form() {
        // PARTITION-TOTAL CONTRACT (value-level): pin that for every
        // reachable Sexp outer shape, EXACTLY ONE of `as_atom`,
        // `as_quote_form`, `as_structural_kind` returns `Some(_)`.
        // Post-lift the three carving-marker projections at the value
        // level form a partition of the `Sexp` variant algebra —
        // symmetric with the partition-total invariant pinned at the
        // shape level by
        // `sexp_shape_partition_is_total_across_atom_quote_structural_carvings`
        // (in `error.rs`). A regression that drifts any carving's
        // membership (an `as_atom` arm that accepts a non-atom, an
        // `as_quote_form` arm that misses a quote-family wrapper, an
        // `as_structural_kind` arm that swaps its Nil/List
        // membership) surfaces here immediately, so the value-level
        // partition invariant is a TYPED THEOREM (rustc-enforced
        // exhaustiveness through the joint sweep) rather than a
        // runtime `matches!` assertion.
        let samples = [
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ];
        for s in &samples {
            let hits = [
                s.as_atom().is_some(),
                s.as_quote_form().is_some(),
                s.as_structural_kind().is_some(),
            ];
            let hit_count: usize = hits.iter().filter(|b| **b).count();
            assert_eq!(
                hit_count, 1,
                "value-level carvings must partition Sexp variants — {s:?} matched {hit_count} carvings (as_atom/as_quote_form/as_structural_kind = {hits:?})",
            );
        }
    }

    #[test]
    fn sexp_as_structural_kind_composes_with_label_via_structural_kind_label() {
        // CROSS-PROJECTION COHERENCE: pin that
        // `s.as_structural_kind().map(StructuralKind::label)` agrees
        // with `s.shape().label()` for every residual-carving Sexp
        // (and returns `None` for every non-residual Sexp). Composes
        // the new value-level projection with the closed-set
        // `StructuralKind::label` projection (which itself composes
        // through `sexp_shape().label()`) so the label vocabulary
        // stays load-bearing at ONE canonical site
        // (`SexpShape::label`) rather than a parallel per-projection
        // literal table.
        let residual = [
            (Sexp::Nil, "nil"),
            (Sexp::List(vec![]), "list"),
            (Sexp::List(vec![Sexp::symbol("a")]), "list"),
        ];
        for (sexp, expected_label) in &residual {
            let via_carving = sexp.as_structural_kind().map(StructuralKind::label);
            assert_eq!(
                via_carving,
                Some(*expected_label),
                "structural-carving-marker label drifted at {sexp:?}"
            );
            assert_eq!(
                via_carving,
                Some(sexp.shape().label()),
                "as_structural_kind.map(label) must equal shape().label() for residual sample {sexp:?}"
            );
        }
        for non_residual in [
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ] {
            assert_eq!(
                non_residual.as_structural_kind().map(StructuralKind::label),
                None,
                "non-residual Sexp must project to None on as_structural_kind.map(label) — {non_residual:?}"
            );
        }
    }

    #[test]
    fn sexp_as_atom_kind_projects_each_atom_variant_to_canonical_atom_kind() {
        // PER-VARIANT TRUTH-TABLE (atomic axis): pin byte-for-byte per-
        // Sexp-atom-arm mapping — Symbol payload → Some(AtomKind::Symbol),
        // Keyword payload → Some(AtomKind::Keyword), Str payload →
        // Some(AtomKind::Str), Int payload → Some(AtomKind::Int), Float
        // payload → Some(AtomKind::Float), Bool payload →
        // Some(AtomKind::Bool). Value-level peer of the shape-level
        // sweep `as_atom_kind_projects_each_atom_shape_to_canonical_atom_kind_and_rejects_non_atom_shapes`
        // in error.rs — each atomic Sexp value's carving-marker
        // projection must land on the matching AtomKind arm the shape-
        // level projection lands on. A future thirteenth Atom variant
        // extends both this sweep + the composition body via the
        // as_atom + Atom::kind primitives, with rustc enforcing the
        // match arms in lockstep.
        assert_eq!(Sexp::symbol("foo").as_atom_kind(), Some(AtomKind::Symbol));
        assert_eq!(Sexp::keyword("k").as_atom_kind(), Some(AtomKind::Keyword));
        assert_eq!(Sexp::string("s").as_atom_kind(), Some(AtomKind::Str));
        assert_eq!(Sexp::int(7).as_atom_kind(), Some(AtomKind::Int));
        assert_eq!(Sexp::float(7.5).as_atom_kind(), Some(AtomKind::Float));
        assert_eq!(Sexp::boolean(true).as_atom_kind(), Some(AtomKind::Bool));
        // Empty-payload edge cases (empty-string vs Symbol vs Keyword)
        // — pin the projection ignores payload content entirely (it
        // reads only the outer variant discriminant), so a body that
        // gates on payload emptiness fails loudly.
        assert_eq!(Sexp::symbol("").as_atom_kind(), Some(AtomKind::Symbol));
        assert_eq!(Sexp::string("").as_atom_kind(), Some(AtomKind::Str));
    }

    #[test]
    fn sexp_as_atom_kind_rejects_non_atom_outer_shapes() {
        // KERNEL: every non-atom outer shape (Nil, List, every quote-
        // family wrapper) projects to `None`. Sibling kernel-pin to
        // `sexp_as_structural_kind_rejects_non_structural_outer_shapes`
        // on the residual axis. Together the two kernel pins bracket
        // the atomic-carving membership from BOTH sides of the
        // partition — the atomic-arm membership from
        // `sexp_as_atom_kind_projects_each_atom_variant_to_canonical_atom_kind`
        // and the non-atomic-arm kernel from THIS test.
        for non_atom in [
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ] {
            assert_eq!(
                non_atom.as_atom_kind(),
                None,
                "non-atom Sexp must project to None on as_atom_kind — {non_atom:?}"
            );
        }
    }

    #[test]
    fn sexp_as_atom_kind_agrees_with_as_atom_map_kind_for_every_variant() {
        // COMPOSITION-LAW CONTRACT (atomic-axis peer of the shape-
        // agreement law): `s.as_atom_kind() == s.as_atom().map(Atom::kind)`
        // for every reachable Sexp outer shape. Pre-lift the atomic
        // carving marker at the value level was reachable via this
        // two-step composition through the Atom algebra; post-lift the
        // new projection MUST agree bit-for-bit — the substrate's
        // (Sexp value, AtomKind marker) pairing binds at TWO
        // compositions (this Atom-axis composition AND the shape-axis
        // composition pinned by the sibling test below) that must stay
        // in lockstep. Sweeps every outer shape (atom + residual +
        // quote-family) so a drift on ANY arm surfaces immediately.
        let samples = [
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ];
        for s in &samples {
            assert_eq!(
                s.as_atom_kind(),
                s.as_atom().map(Atom::kind),
                "Sexp::as_atom_kind and Sexp::as_atom().map(Atom::kind) must agree at {s:?}",
            );
        }
    }

    #[test]
    fn sexp_as_atom_kind_agrees_with_shape_as_atom_kind_for_every_variant() {
        // COMPOSITION-LAW CONTRACT (shape-axis peer): `s.as_atom_kind()
        // == s.shape().as_atom_kind()` for every reachable Sexp outer
        // shape. Sibling to
        // `sexp_as_structural_kind_agrees_with_shape_as_structural_kind_for_every_variant`
        // on the atomic axis. Pre-lift the atomic carving marker at
        // the value level was reachable via this two-step composition
        // through the shape algebra; post-lift the new projection MUST
        // agree bit-for-bit — the substrate's (Sexp value, AtomKind
        // marker) pairing binds at THREE typed methods (Sexp::as_atom_kind,
        // Sexp::as_atom + Atom::kind composition, Sexp::shape +
        // SexpShape::as_atom_kind composition) that must ALL stay in
        // lockstep.
        let samples = [
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ];
        for s in &samples {
            assert_eq!(
                s.as_atom_kind(),
                s.shape().as_atom_kind(),
                "Sexp::as_atom_kind and Sexp::shape().as_atom_kind must agree at {s:?}",
            );
        }
    }

    #[test]
    fn sexp_as_atom_kind_partitions_outer_shapes_jointly_with_as_quote_form_and_as_structural_kind()
    {
        // PARTITION-TOTAL CONTRACT (value-level, marker-only axis):
        // pin that for every reachable Sexp outer shape, EXACTLY ONE
        // of `as_atom_kind`, `as_quote_form`, `as_structural_kind`
        // returns `Some(_)`. Post-lift ALL THREE carving-marker
        // projections at the value level form a partition of the
        // `Sexp` variant algebra using ONLY the marker-only siblings —
        // symmetric with the shape-level partition-total invariant
        // pinned by
        // `sexp_shape_partition_is_total_across_atom_quote_structural_carvings`
        // (in error.rs). The pre-existing value-level partition pin
        // `sexp_as_structural_kind_partitions_outer_shapes_jointly_with_as_atom_and_as_quote_form`
        // uses `as_atom().is_some()` on the atomic axis (the
        // structural-lift projection); THIS pin uses `as_atom_kind()
        // .is_some()` (the marker-only projection). Both partition
        // invariants must hold — they pin the atomic axis's TWO
        // value-level projections (structural + marker) as jointly
        // partition-consistent with the residual and quote-family
        // siblings. A regression that drifts any carving's
        // marker-only membership (an `as_atom_kind` arm that accepts
        // a non-atom, an `as_quote_form` arm that misses a quote-
        // family wrapper, an `as_structural_kind` arm that swaps its
        // Nil/List membership) surfaces here immediately.
        let samples = [
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ];
        for s in &samples {
            let hits = [
                s.as_atom_kind().is_some(),
                s.as_quote_form().is_some(),
                s.as_structural_kind().is_some(),
            ];
            let hit_count: usize = hits.iter().filter(|b| **b).count();
            assert_eq!(
                hit_count, 1,
                "value-level marker-only carvings must partition Sexp variants — {s:?} matched {hit_count} carvings (as_atom_kind/as_quote_form/as_structural_kind = {hits:?})",
            );
        }
    }

    #[test]
    fn sexp_as_atom_kind_composes_with_label_via_atom_kind_label() {
        // CROSS-PROJECTION COHERENCE: pin that
        // `s.as_atom_kind().map(AtomKind::label)` agrees with
        // `s.shape().label()` for every atomic Sexp (and returns
        // `None` for every non-atomic Sexp). Sibling to
        // `sexp_as_structural_kind_composes_with_label_via_structural_kind_label`
        // on the atomic axis. Composes the new value-level marker
        // projection with the closed-set `AtomKind::label` projection
        // (which itself composes through `sexp_shape().label()`) so
        // the label vocabulary stays load-bearing at ONE canonical
        // site (`SexpShape::label`) rather than a parallel per-
        // projection literal table.
        let atomic = [
            (Sexp::symbol("foo"), "symbol"),
            (Sexp::keyword("k"), "keyword"),
            (Sexp::string("s"), "string"),
            (Sexp::int(7), "int"),
            (Sexp::float(7.5), "float"),
            (Sexp::boolean(true), "bool"),
        ];
        for (sexp, expected_label) in &atomic {
            let via_carving = sexp.as_atom_kind().map(AtomKind::label);
            assert_eq!(
                via_carving,
                Some(*expected_label),
                "atomic-carving-marker label drifted at {sexp:?}"
            );
            assert_eq!(
                via_carving,
                Some(sexp.shape().label()),
                "as_atom_kind.map(label) must equal shape().label() for atomic sample {sexp:?}"
            );
        }
        for non_atomic in [
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ] {
            assert_eq!(
                non_atomic.as_atom_kind().map(AtomKind::label),
                None,
                "non-atomic Sexp must project to None on as_atom_kind.map(label) — {non_atomic:?}"
            );
        }
    }

    #[test]
    fn sexp_as_unquote_form_projects_each_variant_to_canonical_unquote_form() {
        // PER-VARIANT TRUTH-TABLE (unquote-subset axis): pin byte-for-
        // byte per-Sexp-substitution-arm mapping — `Sexp::Unquote(inner)`
        // → `Some(UnquoteForm::Unquote)`, `Sexp::UnquoteSplice(inner)`
        // → `Some(UnquoteForm::Splice)`. Value-level peer of the shape-
        // level sweep
        // `as_unquote_form_projects_each_unquote_shape_to_canonical_unquote_form_and_rejects_non_unquote_shapes`
        // in error.rs — each substitution-wrapper Sexp value's carving-
        // marker projection must land on the matching UnquoteForm arm
        // the shape-level projection lands on. A future third UnquoteForm
        // variant (e.g. `,~` reverse-unquote) extends both this sweep +
        // the composition body via the as_unquote + QuoteForm::as_unquote_form
        // primitives, with rustc enforcing the match arms in lockstep.
        assert_eq!(
            Sexp::Unquote(Box::new(Sexp::symbol("x"))).as_unquote_form(),
            Some(UnquoteForm::Unquote)
        );
        assert_eq!(
            Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))).as_unquote_form(),
            Some(UnquoteForm::Splice)
        );
        // Inner-payload invariance edge cases — pin the projection
        // ignores inner payload content entirely (it reads only the
        // outer wrapper variant discriminant), so a body that gates on
        // inner payload shape fails loudly.
        assert_eq!(
            Sexp::Unquote(Box::new(Sexp::Nil)).as_unquote_form(),
            Some(UnquoteForm::Unquote)
        );
        assert_eq!(
            Sexp::UnquoteSplice(Box::new(Sexp::List(vec![]))).as_unquote_form(),
            Some(UnquoteForm::Splice)
        );
        assert_eq!(
            Sexp::Unquote(Box::new(Sexp::List(vec![
                Sexp::symbol("nested"),
                Sexp::int(42),
            ])))
            .as_unquote_form(),
            Some(UnquoteForm::Unquote)
        );
    }

    #[test]
    fn sexp_as_unquote_form_rejects_non_unquote_subset_outer_shapes() {
        // KERNEL: every non-unquote-subset outer shape (Nil, every Atom
        // variant, List, AND the two non-substitution quote-family
        // wrappers `Sexp::Quote` and `Sexp::Quasiquote`) projects to
        // `None`. Sibling kernel-pin to
        // `sexp_as_atom_kind_rejects_non_atom_outer_shapes` and
        // `sexp_as_structural_kind_rejects_non_structural_outer_shapes`
        // on the substitution axis. The two non-substitution quote-
        // family wrappers ARE quote-family (`as_quote_form` accepts
        // them) but NOT substitution-subset (`as_unquote_form` must
        // reject them) — pin the 2-of-4 subset gate operates at the
        // value level exactly as the shape-level
        // `QuoteForm::as_unquote_form` gate operates on the closed-set
        // marker enum.
        for non_unquote in [
            Sexp::Nil,
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
        ] {
            assert_eq!(
                non_unquote.as_unquote_form(),
                None,
                "non-substitution-subset Sexp must project to None on as_unquote_form — {non_unquote:?}"
            );
        }
    }

    #[test]
    fn sexp_as_unquote_form_agrees_with_as_unquote_map_marker_for_every_variant() {
        // COMPOSITION-LAW CONTRACT (parent-projection peer): pin
        // `s.as_unquote_form() == s.as_unquote().map(|(uf, _)| uf)` for
        // every reachable Sexp outer shape. Pre-lift the substitution
        // carving marker at the value level was reachable via this
        // two-step composition through the parent [`Sexp::as_unquote`]
        // projection (discarding the wrapped inner); post-lift the new
        // marker-only projection MUST agree bit-for-bit — the
        // substrate's (Sexp value, UnquoteForm marker) pairing binds at
        // FOUR compositions (this parent-projection composition AND the
        // shape-axis composition AND the quote-family + subset-gate
        // composition, all pinned by the sibling tests below) that must
        // stay in lockstep. Sweeps every outer shape (atom + residual +
        // quote-family) so a drift on ANY arm surfaces immediately.
        let samples = [
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ];
        for s in &samples {
            assert_eq!(
                s.as_unquote_form(),
                s.as_unquote().map(|(uf, _)| uf),
                "Sexp::as_unquote_form and Sexp::as_unquote().map(|(uf, _)| uf) must agree at {s:?}",
            );
        }
    }

    #[test]
    fn sexp_as_unquote_form_agrees_with_shape_as_unquote_form_for_every_variant() {
        // COMPOSITION-LAW CONTRACT (shape-axis peer): `s.as_unquote_form()
        // == s.shape().as_unquote_form()` for every reachable Sexp outer
        // shape. Sibling to
        // `sexp_as_atom_kind_agrees_with_shape_as_atom_kind_for_every_variant`
        // and
        // `sexp_as_structural_kind_agrees_with_shape_as_structural_kind_for_every_variant`
        // on the substitution axis. Pre-lift the substitution carving
        // marker at the value level was reachable via this two-step
        // composition through the shape algebra; post-lift the new
        // projection MUST agree bit-for-bit — the substrate's (Sexp
        // value, UnquoteForm marker) pairing binds at FOUR typed methods
        // (Sexp::as_unquote_form, Sexp::as_unquote + `|(uf, _)| uf`
        // composition, Sexp::shape + SexpShape::as_unquote_form
        // composition, Sexp::as_quote_form +
        // QuoteForm::as_unquote_form composition) that must ALL stay
        // in lockstep.
        let samples = [
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ];
        for s in &samples {
            assert_eq!(
                s.as_unquote_form(),
                s.shape().as_unquote_form(),
                "Sexp::as_unquote_form and Sexp::shape().as_unquote_form must agree at {s:?}",
            );
        }
    }

    #[test]
    fn sexp_as_unquote_form_agrees_with_as_quote_form_and_quote_form_as_unquote_form_for_every_variant(
    ) {
        // COMPOSITION-LAW CONTRACT (parent-family + subset-gate peer):
        // pin `s.as_unquote_form() ==
        // s.as_quote_form().and_then(|(qf, _)| qf.as_unquote_form())`
        // for every reachable Sexp outer shape. Value-level peer of the
        // shape-level route
        // `as_unquote_form_routes_through_as_quote_form_and_quote_form_as_unquote_form_via_composition`
        // in error.rs — where that test pins the shape-level
        // `SexpShape::as_unquote_form` routes through the shape-level
        // `SexpShape::as_quote_form` + `QuoteForm::as_unquote_form`
        // subset gate, THIS test pins the value-level
        // `Sexp::as_unquote_form` routes through the value-level
        // `Sexp::as_quote_form` + the SAME subset gate. Pre-lift the
        // substitution carving marker at the value level was reachable
        // via this three-step composition through the parent quote-
        // family projection [`Sexp::as_quote_form`] composed with the
        // 2-of-4 subset gate [`QuoteForm::as_unquote_form`]; post-lift
        // the new marker-only projection MUST agree bit-for-bit — the
        // subset-gate composition (which the pre-existing
        // [`Sexp::as_unquote`] projection ALSO routes through, per its
        // body `let (qf, inner) = self.as_quote_form()?;
        // qf.as_unquote_form().map(|uf| (uf, inner))`) must land the
        // same marker as the new value-level projection.
        let samples = [
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ];
        for s in &samples {
            assert_eq!(
                s.as_unquote_form(),
                s.as_quote_form().and_then(|(qf, _)| qf.as_unquote_form()),
                "Sexp::as_unquote_form and Sexp::as_quote_form().and_then(|(qf, _)| qf.as_unquote_form()) must agree at {s:?}",
            );
        }
    }

    #[test]
    fn sexp_as_unquote_form_composes_with_marker_via_unquote_form_marker() {
        // CROSS-PROJECTION COHERENCE: pin that
        // `s.as_unquote_form().map(UnquoteForm::marker)` agrees with
        // `s.shape().label()` for every substitution-subset Sexp (and
        // returns `None` for every non-substitution-subset Sexp).
        // Sibling to `sexp_as_atom_kind_composes_with_label_via_atom_kind_label`
        // on the substitution axis. Composes the new value-level
        // marker projection with the closed-set `UnquoteForm::marker`
        // projection (which itself composes through
        // `to_quote_form().prefix()` — see `UnquoteForm::marker`'s
        // docstring for the composition route) so the marker vocabulary
        // (`","` / `",@"`) stays load-bearing at ONE canonical site
        // (`QuoteForm::prefix`'s Unquote/UnquoteSplice arms) rather
        // than a parallel per-projection literal table.
        //
        // Note: `UnquoteForm::marker` returns the READER prefix (`,` or
        // `,@`) which is ALSO the canonical `SexpShape::label` for the
        // Unquote / UnquoteSplice arms — the shape-label vocabulary
        // was pinned to the reader-prefix vocabulary in the
        // `SexpShape::label` truth-table (Unquote → "unquote",
        // UnquoteSplice → "unquote-splice"). This test uses
        // `UnquoteForm::marker` = reader prefix directly (`,` /
        // `,@`), NOT the shape label — the two are distinct
        // vocabularies, both derived from the closed-set carving
        // marker, both stable across the lift.
        let substitution = [
            (
                Sexp::Unquote(Box::new(Sexp::symbol("x"))),
                UnquoteForm::Unquote,
            ),
            (
                Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))),
                UnquoteForm::Splice,
            ),
        ];
        for (sexp, expected_uf) in &substitution {
            let via_carving = sexp.as_unquote_form().map(UnquoteForm::marker);
            assert_eq!(
                via_carving,
                Some(expected_uf.marker()),
                "substitution-carving-marker string drifted at {sexp:?}"
            );
        }
        for non_substitution in [
            Sexp::Nil,
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
        ] {
            assert_eq!(
                non_substitution.as_unquote_form().map(UnquoteForm::marker),
                None,
                "non-substitution-subset Sexp must project to None on as_unquote_form.map(marker) — {non_substitution:?}"
            );
        }
    }

    #[test]
    fn sexp_as_unquote_form_narrows_as_quote_form_to_substitution_subset() {
        // SUBSET-GATE CONTRACT (value-level): pin that at every
        // reachable Sexp outer shape, `as_unquote_form().is_some()`
        // implies `as_quote_form().is_some()` (subset containment) AND
        // `as_quote_form().is_some() && !as_unquote_form().is_some()`
        // holds exactly for the two non-substitution quote-family
        // wrappers (`Sexp::Quote` and `Sexp::Quasiquote`) — the 2-of-4
        // subset gate at the VALUE level, symmetric with the shape-
        // level subset gate pinned by the sibling
        // `QuoteForm::as_unquote_form` truth-table in error.rs. Pins the
        // (substitution-subset ⊂ quote-family) inclusion as an invariant
        // on the value algebra so a regression that widens
        // `as_unquote_form` beyond its 2-of-4 subset (e.g. an emitter
        // that starts accepting `Sexp::Quote` as substitution) surfaces
        // immediately as a subset-inclusion drift.
        let samples = [
            (Sexp::Nil, false, false),
            (Sexp::List(vec![]), false, false),
            (Sexp::List(vec![Sexp::symbol("a")]), false, false),
            (Sexp::symbol("foo"), false, false),
            (Sexp::keyword("k"), false, false),
            (Sexp::string("s"), false, false),
            (Sexp::int(7), false, false),
            (Sexp::float(7.5), false, false),
            (Sexp::boolean(true), false, false),
            // Quote-family, NOT substitution-subset
            (Sexp::Quote(Box::new(Sexp::Nil)), true, false),
            (Sexp::Quasiquote(Box::new(Sexp::Nil)), true, false),
            // Quote-family AND substitution-subset
            (Sexp::Unquote(Box::new(Sexp::Nil)), true, true),
            (Sexp::UnquoteSplice(Box::new(Sexp::Nil)), true, true),
        ];
        for (s, quote_expected, unquote_expected) in &samples {
            let quote_hit = s.as_quote_form().is_some();
            let unquote_hit = s.as_unquote_form().is_some();
            assert_eq!(
                quote_hit, *quote_expected,
                "as_quote_form membership drifted at {s:?}"
            );
            assert_eq!(
                unquote_hit, *unquote_expected,
                "as_unquote_form membership drifted at {s:?}"
            );
            // Subset containment: substitution ⊂ quote-family.
            assert!(
                !unquote_hit || quote_hit,
                "subset containment violated at {s:?}: as_unquote_form Some but as_quote_form None",
            );
        }
    }

    #[test]
    fn sexp_as_quote_form_marker_projects_each_variant_to_canonical_quote_form() {
        // PER-VARIANT TRUTH-TABLE (quote-family axis): pin byte-for-byte
        // per-Sexp-quote-family-arm mapping — `Sexp::Quote(inner)`
        // → `Some(QuoteForm::Quote)`, `Sexp::Quasiquote(inner)`
        // → `Some(QuoteForm::Quasiquote)`, `Sexp::Unquote(inner)`
        // → `Some(QuoteForm::Unquote)`, `Sexp::UnquoteSplice(inner)`
        // → `Some(QuoteForm::UnquoteSplice)`. Value-level marker-only
        // peer of the pre-existing tuple projection
        // `Sexp::as_quote_form` — each quote-family-wrapper Sexp value's
        // carving-marker projection must land on the matching QuoteForm
        // arm the parent projection's tuple carries. A future fifth
        // QuoteForm variant (e.g. `,~` reverse-unquote) extends both
        // this sweep + the composition body via the as_quote_form
        // primitive, with rustc enforcing the match arms in lockstep.
        assert_eq!(
            Sexp::Quote(Box::new(Sexp::symbol("x"))).as_quote_form_marker(),
            Some(QuoteForm::Quote)
        );
        assert_eq!(
            Sexp::Quasiquote(Box::new(Sexp::symbol("x"))).as_quote_form_marker(),
            Some(QuoteForm::Quasiquote)
        );
        assert_eq!(
            Sexp::Unquote(Box::new(Sexp::symbol("x"))).as_quote_form_marker(),
            Some(QuoteForm::Unquote)
        );
        assert_eq!(
            Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))).as_quote_form_marker(),
            Some(QuoteForm::UnquoteSplice)
        );
        // Inner-payload invariance edge cases — pin the projection
        // ignores inner payload content entirely (it reads only the
        // outer wrapper variant discriminant), so a body that gates on
        // inner payload shape fails loudly.
        assert_eq!(
            Sexp::Quote(Box::new(Sexp::Nil)).as_quote_form_marker(),
            Some(QuoteForm::Quote)
        );
        assert_eq!(
            Sexp::Quasiquote(Box::new(Sexp::List(vec![]))).as_quote_form_marker(),
            Some(QuoteForm::Quasiquote)
        );
        assert_eq!(
            Sexp::Unquote(Box::new(Sexp::List(vec![
                Sexp::symbol("nested"),
                Sexp::int(42),
            ])))
            .as_quote_form_marker(),
            Some(QuoteForm::Unquote)
        );
        assert_eq!(
            Sexp::UnquoteSplice(Box::new(Sexp::Quote(Box::new(Sexp::symbol("y")))))
                .as_quote_form_marker(),
            Some(QuoteForm::UnquoteSplice)
        );
    }

    #[test]
    fn sexp_as_quote_form_marker_rejects_non_quote_family_outer_shapes() {
        // KERNEL: every non-quote-family outer shape (Nil, every Atom
        // variant, List — empty and non-empty) projects to `None`.
        // Sibling kernel-pin to
        // `sexp_as_atom_kind_rejects_non_atom_outer_shapes`,
        // `sexp_as_structural_kind_rejects_non_structural_outer_shapes`,
        // and `sexp_as_unquote_form_rejects_non_unquote_subset_outer_shapes`
        // on the quote-family axis. A body that widens the projection to
        // any non-quote-family arm (e.g. `Sexp::List` starts returning
        // `Some(QuoteForm::Quote)`) surfaces as a `None` expectation
        // failure at the specific offending variant.
        for non_quote in [
            Sexp::Nil,
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::symbol("a"), Sexp::int(1)]),
        ] {
            assert_eq!(
                non_quote.as_quote_form_marker(),
                None,
                "non-quote-family Sexp must project to None on as_quote_form_marker — {non_quote:?}"
            );
        }
    }

    #[test]
    fn sexp_as_quote_form_marker_agrees_with_as_quote_form_map_marker_for_every_variant() {
        // COMPOSITION-LAW CONTRACT (parent-projection peer): pin
        // `s.as_quote_form_marker() == s.as_quote_form().map(|(qf, _)| qf)`
        // for every reachable Sexp outer shape. Pre-lift the quote-
        // family carving marker at the value level was reachable via
        // this two-step composition through the parent
        // [`Sexp::as_quote_form`] projection (discarding the wrapped
        // inner via `.map(|(qf, _)| qf)`); post-lift the new marker-
        // only projection MUST agree bit-for-bit — the substrate's
        // (Sexp value, QuoteForm marker) pairing binds at THREE
        // compositions (this parent-projection composition AND the
        // shape-axis composition, both pinned in this module, AND the
        // direct match in the new method's body) that must stay in
        // lockstep. Sweeps every outer shape (atom + residual +
        // quote-family) so a drift on ANY arm surfaces immediately.
        let samples = [
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ];
        for s in &samples {
            assert_eq!(
                s.as_quote_form_marker(),
                s.as_quote_form().map(|(qf, _)| qf),
                "Sexp::as_quote_form_marker and Sexp::as_quote_form().map(|(qf, _)| qf) must agree at {s:?}",
            );
        }
    }

    #[test]
    fn sexp_as_quote_form_marker_agrees_with_shape_as_quote_form_for_every_variant() {
        // COMPOSITION-LAW CONTRACT (shape-axis peer):
        // `s.as_quote_form_marker() == s.shape().as_quote_form()` for
        // every reachable Sexp outer shape. Sibling to
        // `sexp_as_atom_kind_agrees_with_shape_as_atom_kind_for_every_variant`,
        // `sexp_as_structural_kind_agrees_with_shape_as_structural_kind_for_every_variant`,
        // and `sexp_as_unquote_form_agrees_with_shape_as_unquote_form_for_every_variant`
        // on the quote-family axis. Pre-lift the quote-family carving
        // marker at the value level was reachable via this two-step
        // composition through the shape algebra (`shape().as_quote_form()`,
        // walking the full 12-variant [`SexpShape`](crate::error::SexpShape)
        // closed set to arrive at the 4-of-12 carving marker); post-
        // lift the new projection MUST agree bit-for-bit — the
        // substrate's (Sexp value, QuoteForm marker) pairing now binds
        // at ONE typed method on the value algebra, with both
        // compositions (this shape-axis peer and the parent-projection
        // peer pinned above) staying in lockstep.
        use crate::error::SexpShape;
        let samples = [
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ];
        for s in &samples {
            let via_value: Option<QuoteForm> = s.as_quote_form_marker();
            let via_shape: Option<QuoteForm> = SexpShape::as_quote_form(s.shape());
            assert_eq!(
                via_value, via_shape,
                "Sexp::as_quote_form_marker and Sexp::shape().as_quote_form must agree at {s:?}",
            );
        }
    }

    #[test]
    fn sexp_as_quote_form_marker_composes_with_prefix_via_quote_form_prefix() {
        // CROSS-PROJECTION COHERENCE: pin that
        // `s.as_quote_form_marker().map(QuoteForm::prefix)` agrees with
        // the reader-prefix vocabulary carried on [`QuoteForm::prefix`]
        // for every quote-family Sexp (and returns `None` for every
        // non-quote-family Sexp). Sibling to
        // `sexp_as_unquote_form_composes_with_marker_via_unquote_form_marker`
        // on the quote-family axis. Composes the new value-level marker
        // projection with the closed-set [`QuoteForm::prefix`]
        // projection so the reader/writer prefix vocabulary (`'` / `` ` ``
        // / `,` / `,@`) stays load-bearing at ONE canonical site
        // ([`QuoteForm::prefix`]'s four arms) rather than a parallel
        // per-projection literal table on the value algebra.
        let quote_family = [
            (Sexp::Quote(Box::new(Sexp::symbol("x"))), QuoteForm::Quote),
            (
                Sexp::Quasiquote(Box::new(Sexp::symbol("x"))),
                QuoteForm::Quasiquote,
            ),
            (
                Sexp::Unquote(Box::new(Sexp::symbol("x"))),
                QuoteForm::Unquote,
            ),
            (
                Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))),
                QuoteForm::UnquoteSplice,
            ),
        ];
        for (sexp, expected_qf) in &quote_family {
            let via_carving = sexp.as_quote_form_marker().map(QuoteForm::prefix);
            assert_eq!(
                via_carving,
                Some(expected_qf.prefix()),
                "quote-family carving-marker prefix drifted at {sexp:?}"
            );
        }
        for non_quote in [
            Sexp::Nil,
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("a")]),
        ] {
            assert_eq!(
                non_quote.as_quote_form_marker().map(QuoteForm::prefix),
                None,
                "non-quote-family Sexp must project to None on as_quote_form_marker.map(prefix) — {non_quote:?}"
            );
        }
    }

    #[test]
    fn sexp_as_quote_form_marker_extends_as_unquote_form_to_full_quote_family() {
        // SUPERSET-GATE CONTRACT (value-level): pin that at every
        // reachable Sexp outer shape,
        // `as_unquote_form().is_some()` implies
        // `as_quote_form_marker().is_some()` (the 2-of-12 substitution
        // subset is a proper subset of the 4-of-12 quote family) AND
        // `as_quote_form_marker().is_some() && !as_unquote_form().is_some()`
        // holds exactly for the two non-substitution quote-family
        // wrappers (`Sexp::Quote` and `Sexp::Quasiquote`) — the value-
        // level image of the 2-of-4 subset gate
        // [`QuoteForm::as_unquote_form`], mirroring
        // `sexp_as_unquote_form_narrows_as_quote_form_to_substitution_subset`
        // from the substitution-axis side. Pins the (substitution-
        // subset ⊂ quote-family) inclusion as an invariant on the
        // value algebra where the SUPERSET side is now a NAMED typed
        // method — so a regression that widens either projection
        // beyond its cell (e.g. `as_quote_form_marker` starts accepting
        // `Sexp::List`, or `as_unquote_form` starts accepting
        // `Sexp::Quote`) surfaces immediately as a subset-inclusion
        // drift. Also pin that
        // `as_unquote_form() == as_quote_form_marker().and_then(
        //     QuoteForm::as_unquote_form)` — the value-level projection
        // composes with the 2-of-4 subset gate at the marker algebra
        // level, so the substrate's (Sexp value, UnquoteForm marker)
        // pairing derives from the (Sexp value, QuoteForm marker)
        // pairing at ONE composition rather than two parallel value-
        // level projections.
        let samples = [
            (Sexp::Nil, false, false),
            (Sexp::List(vec![]), false, false),
            (Sexp::List(vec![Sexp::symbol("a")]), false, false),
            (Sexp::symbol("foo"), false, false),
            (Sexp::keyword("k"), false, false),
            (Sexp::string("s"), false, false),
            (Sexp::int(7), false, false),
            (Sexp::float(7.5), false, false),
            (Sexp::boolean(true), false, false),
            // Quote-family, NOT substitution-subset
            (Sexp::Quote(Box::new(Sexp::Nil)), true, false),
            (Sexp::Quasiquote(Box::new(Sexp::Nil)), true, false),
            // Quote-family AND substitution-subset
            (Sexp::Unquote(Box::new(Sexp::Nil)), true, true),
            (Sexp::UnquoteSplice(Box::new(Sexp::Nil)), true, true),
        ];
        for (s, quote_expected, unquote_expected) in &samples {
            let quote_hit = s.as_quote_form_marker().is_some();
            let unquote_hit = s.as_unquote_form().is_some();
            assert_eq!(
                quote_hit, *quote_expected,
                "as_quote_form_marker membership drifted at {s:?}"
            );
            assert_eq!(
                unquote_hit, *unquote_expected,
                "as_unquote_form membership drifted at {s:?}"
            );
            // Superset containment: substitution ⊂ quote-family.
            assert!(
                !unquote_hit || quote_hit,
                "subset containment violated at {s:?}: as_unquote_form Some but as_quote_form_marker None",
            );
            // Composition through the 2-of-4 subset gate:
            // `s.as_unquote_form() == s.as_quote_form_marker().and_then(QuoteForm::as_unquote_form)`.
            assert_eq!(
                s.as_unquote_form(),
                s.as_quote_form_marker()
                    .and_then(QuoteForm::as_unquote_form),
                "as_unquote_form and as_quote_form_marker + QuoteForm::as_unquote_form composition disagree at {s:?}",
            );
        }
    }

    #[test]
    fn sexp_shape_method_label_composes_with_sexp_type_name_for_every_outer_shape() {
        // COMPOSITION-LAW CONTRACT: `s.shape().label() ==
        // crate::domain::sexp_type_name(&s)` for every reachable Sexp
        // outer shape. Post-lift `sexp_type_name` routes through
        // `s.shape().label()` directly (no longer through the free-
        // function `sexp_shape`). Pin the composition law so a future
        // refactor that drifts either projection (e.g. a label typo
        // in `SexpShape::label`, a change in `sexp_type_name`'s
        // delegation) surfaces here immediately.
        let samples = [
            Sexp::Nil,
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::List(vec![]),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ];
        for s in &samples {
            assert_eq!(
                s.shape().label(),
                crate::domain::sexp_type_name(s),
                "Sexp::shape().label() must equal domain::sexp_type_name for {s:?}",
            );
        }
    }

    #[test]
    fn sexp_type_name_method_projects_each_outer_arm_to_canonical_label() {
        // PER-ARM CONTRACT: pin that the inherent `Sexp::type_name()`
        // method projects each reachable outer Sexp shape to its
        // canonical `&'static str` label. Pre-lift the projection
        // lived as a free function `domain::sexp_type_name`; post-
        // lift the canonical site is the inherent method on the
        // `Sexp` algebra and the free function delegates. A
        // regression that drifts a per-arm label (e.g. a typo in
        // `SexpShape::label`, a stale arm in `Sexp::shape`'s match,
        // a change in the body away from `self.shape().label()`)
        // surfaces here immediately. Sweeps every outer shape and
        // every atomic payload kind so all 8 `SexpShape` variants
        // are covered.
        assert_eq!(Sexp::Nil.type_name(), "nil");
        assert_eq!(Sexp::symbol("foo").type_name(), "symbol");
        assert_eq!(Sexp::keyword("k").type_name(), "keyword");
        assert_eq!(Sexp::string("s").type_name(), "string");
        assert_eq!(Sexp::int(7).type_name(), "int");
        assert_eq!(Sexp::float(7.5).type_name(), "float");
        assert_eq!(Sexp::boolean(true).type_name(), "bool");
        assert_eq!(Sexp::List(vec![]).type_name(), "list");
        assert_eq!(Sexp::Quote(Box::new(Sexp::Nil)).type_name(), "quote");
        assert_eq!(
            Sexp::Quasiquote(Box::new(Sexp::Nil)).type_name(),
            "quasiquote",
        );
        assert_eq!(Sexp::Unquote(Box::new(Sexp::Nil)).type_name(), "unquote");
        assert_eq!(
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)).type_name(),
            "unquote-splice",
        );
    }

    #[test]
    fn sexp_type_name_method_composes_through_shape_label_for_every_outer_shape() {
        // COMPOSITION-LAW CONTRACT: `s.type_name() == s.shape().label()`
        // for every reachable Sexp outer shape — the method body is
        // structurally derived through `Self::shape` + `SexpShape::label`
        // rather than re-matching `Sexp` arms directly. Pin the
        // composition law so a future refactor that re-inlines the
        // match (and gains its own drift surface) surfaces here
        // immediately. Sibling-shape pin to the existing
        // `sexp_shape_method_label_composes_with_sexp_type_name_for_every_outer_shape`
        // pin (which pins the inverse direction: the free function
        // routes through the inherent method).
        let samples = [
            Sexp::Nil,
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::List(vec![]),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ];
        for s in &samples {
            assert_eq!(
                s.type_name(),
                s.shape().label(),
                "Sexp::type_name() must compose through Sexp::shape().label() for {s:?}",
            );
        }
    }

    #[test]
    fn sexp_type_name_method_agrees_with_domain_sexp_type_name_for_every_outer_shape() {
        // LIFTED-BOUNDARY CONTRACT: pin that the inherent
        // `Sexp::type_name()` method agrees with the free-function
        // delegate `crate::domain::sexp_type_name` for every
        // reachable outer shape. Pre-lift the dispatcher lived as a
        // free function in `domain.rs`; post-lift the canonical site
        // is the inherent method on the `Sexp` algebra and the free
        // function is a one-line delegate. Pin that the delegation
        // stays byte-for-byte equivalent across every outer arm so
        // a regression where the free function drifts from the
        // inherent method (or vice versa) surfaces here immediately.
        // Mirrors `sexp_witness_method_agrees_with_domain_sexp_witness_for_every_outer_shape`
        // for the canonical-label-only peer projection.
        let samples = [
            Sexp::Nil,
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::int(-1),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::boolean(false),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("op"), Sexp::int(1), Sexp::int(2)]),
            Sexp::Quote(Box::new(Sexp::symbol("payload"))),
            Sexp::Quasiquote(Box::new(Sexp::List(vec![Sexp::symbol("foo")]))),
            Sexp::Unquote(Box::new(Sexp::symbol("x"))),
            Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))),
        ];
        for s in &samples {
            assert_eq!(
                s.type_name(),
                crate::domain::sexp_type_name(s),
                "Sexp::type_name() must equal domain::sexp_type_name for {s:?}",
            );
        }
    }

    #[test]
    fn sexp_witness_method_pairs_shape_with_display_for_every_outer_shape() {
        // LIFTED-BOUNDARY CONTRACT: pin that the inherent
        // `Sexp::witness()` method projects each reachable outer Sexp
        // shape to a `SexpWitness` whose `shape` field equals
        // `s.shape()` AND whose `display` field equals
        // `s.to_string()` for every variant + payload combination.
        // Pre-lift the projection lived as a free function in
        // `domain.rs`; post-lift the canonical site is the inherent
        // method on the `Sexp` algebra. A regression where the method
        // drifts EITHER half of the joint identity (a stale `shape`
        // projection that re-inlines without composing through
        // `Sexp::shape`, a `display` projection that diverges from
        // `Sexp::Display`) surfaces here immediately. Sweeps every
        // outer shape, every atomic payload kind, and every
        // quote-family wrapper.
        let samples = [
            Sexp::Nil,
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::int(-1),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::boolean(false),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("op"), Sexp::int(1), Sexp::int(2)]),
            Sexp::Quote(Box::new(Sexp::symbol("payload"))),
            Sexp::Quasiquote(Box::new(Sexp::List(vec![Sexp::symbol("foo")]))),
            Sexp::Unquote(Box::new(Sexp::symbol("x"))),
            Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))),
        ];
        for s in &samples {
            let w = s.witness();
            assert_eq!(
                w.shape,
                s.shape(),
                "Sexp::witness().shape drifted from Sexp::shape() for {s:?}",
            );
            assert_eq!(
                w.display,
                s.to_string(),
                "Sexp::witness().display drifted from Sexp::Display for {s:?}",
            );
        }
    }

    #[test]
    fn sexp_witness_method_agrees_with_domain_sexp_witness_for_every_outer_shape() {
        // LIFTED-BOUNDARY CONTRACT: pin that the inherent
        // `Sexp::witness()` method agrees with the free-function
        // delegate `crate::domain::sexp_witness` for every reachable
        // outer shape. Pre-lift the dispatcher lived as a free
        // function in `domain.rs`; post-lift the canonical site is
        // the inherent method on the `Sexp` algebra and the free
        // function is a one-line delegate. Pin that the delegation
        // stays byte-for-byte equivalent across every outer arm so
        // a regression where the free function drifts from the
        // inherent method (or vice versa) surfaces here immediately.
        // Mirrors `sexp_shape_method_agrees_with_domain_sexp_shape_for_every_outer_shape`
        // for the joint-identity peer projection. Catches a future
        // "consolidation" that removes the free function without
        // updating the method, or vice versa.
        let samples = [
            Sexp::Nil,
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::int(-1),
            Sexp::float(7.5),
            Sexp::float(0.0),
            Sexp::boolean(true),
            Sexp::boolean(false),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("op"), Sexp::int(1), Sexp::int(2)]),
            Sexp::Quote(Box::new(Sexp::symbol("payload"))),
            Sexp::Quasiquote(Box::new(Sexp::List(vec![Sexp::symbol("foo")]))),
            Sexp::Unquote(Box::new(Sexp::symbol("x"))),
            Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))),
        ];
        for s in &samples {
            let via_method = s.witness();
            let via_delegate = crate::domain::sexp_witness(s);
            assert_eq!(
                via_method.shape, via_delegate.shape,
                "Sexp::witness().shape drifted from domain::sexp_witness().shape at {s:?}",
            );
            assert_eq!(
                via_method.display, via_delegate.display,
                "Sexp::witness().display drifted from domain::sexp_witness().display at {s:?}",
            );
        }
    }

    #[test]
    fn sexp_witness_method_routes_through_shape_and_display_projections() {
        // PATH-UNIFORMITY CONTRACT: the lifted `Sexp::witness()` body
        // composes the two algebra-level projections `Sexp::shape()`
        // (structural identity) + `Sexp::Display` (renderable
        // identity) into ONE `SexpWitness::new(shape, display)`
        // value. Pin that the composition agrees bit-for-bit with
        // the direct `SexpWitness::new(s.shape(), s.to_string())`
        // construction across a sweep covering every outer shape.
        // A regression in EITHER projection direction (a
        // `Sexp::witness` arm that bypasses `Sexp::shape` and
        // re-inlines the dispatch, a `Sexp::witness` arm that
        // bypasses `Sexp::Display` and re-formats the literal) is
        // structurally impossible — the typed joint primitive
        // composes through the typed primitive halves once.
        // Sibling shape to `sexp_shape_method_routes_atom_arm_through_atom_kind_sexp_shape_projection`
        // for the joint-identity axis.
        let samples = [
            Sexp::Nil,
            Sexp::symbol("x"),
            Sexp::keyword("kw"),
            Sexp::string("text"),
            Sexp::int(0),
            Sexp::float(2.5),
            Sexp::boolean(true),
            Sexp::List(vec![Sexp::symbol("f"), Sexp::int(1)]),
            Sexp::Quote(Box::new(Sexp::symbol("q"))),
            Sexp::Quasiquote(Box::new(Sexp::symbol("qq"))),
            Sexp::Unquote(Box::new(Sexp::symbol("uq"))),
            Sexp::UnquoteSplice(Box::new(Sexp::symbol("uqs"))),
        ];
        for s in &samples {
            let via_method = s.witness();
            let via_composed = crate::error::SexpWitness::new(s.shape(), s.to_string());
            assert_eq!(
                via_method.shape, via_composed.shape,
                "Sexp::witness drifted shape from SexpWitness::new(s.shape(), s.to_string()) at {s:?}",
            );
            assert_eq!(
                via_method.display, via_composed.display,
                "Sexp::witness drifted display from SexpWitness::new(s.shape(), s.to_string()) at {s:?}",
            );
        }
    }

    #[test]
    fn sexp_witness_distinguishes_int_atom_from_symbol_with_identical_display() {
        // STRUCTURAL-IDENTITY CONTRACT: `Sexp::int(5)` and
        // `Sexp::symbol("5")` Display-render identically (`"5"`) but
        // are STRUCTURALLY DISTINCT — one is `SexpShape::Int`, the
        // other is `SexpShape::Symbol`. Pin that `Sexp::witness()`
        // carries the structural identity through the `shape` slot
        // so the rejection diagnostic distinguishes the two even
        // when the rendered literal collides. Mirrors the
        // free-function-delegate sibling test
        // `sexp_witness_distinguishes_int_atom_from_symbol_with_same_display`
        // in `domain.rs::tests` — that test pins the delegate; this
        // one pins the inherent method on the algebra. Both stay
        // load-bearing across the lifted boundary.
        let w_int = Sexp::int(5).witness();
        let w_sym = Sexp::symbol("5").witness();
        assert_eq!(
            w_int.display, w_sym.display,
            "display collision precondition"
        );
        assert_ne!(
            w_int.shape, w_sym.shape,
            "Sexp::witness must distinguish Int from Symbol via shape even when display collides",
        );
        assert_eq!(w_int.shape, crate::error::SexpShape::Int);
        assert_eq!(w_sym.shape, crate::error::SexpShape::Symbol);
    }

    #[test]
    fn sexp_as_x_family_routes_through_atom_as_x_for_every_atomic_variant() {
        // LIFTED-BOUNDARY CONTRACT: pin that the six `Sexp::as_X`
        // consumer-side projections equal the two-step composition
        // `s.as_atom().and_then(Atom::as_X)` for every atomic payload
        // variant. Pre-lift the six methods opened the same `Self::Atom
        // (Atom::X(s)) => Some(s)` inline arm; post-lift they delegate
        // through the typed projection family on the closed-set `Atom`
        // algebra. A regression that drifts the outer arm (e.g. re-
        // inlines one variant's match without updating the typed
        // projection) surfaces as an inequality here. Sweeps every
        // atomic variant + every consumer projection, AND pins the
        // `Sexp::as_float` widening specialization (`Atom::Int(n)` →
        // `Some(n as f64)`) lives at the consumer layer.
        let cases: &[Atom] = &[
            Atom::Symbol("name".into()),
            Atom::Keyword("kw".into()),
            Atom::Str("body".into()),
            Atom::Int(42),
            Atom::Int(-7),
            Atom::Float(1.5),
            Atom::Float(1.0),
            Atom::Bool(true),
            Atom::Bool(false),
        ];
        for atom in cases {
            let sexp = Sexp::Atom(atom.clone());

            assert_eq!(
                sexp.as_symbol(),
                sexp.as_atom().and_then(Atom::as_symbol),
                "Sexp::as_symbol drifted from as_atom().and_then(Atom::as_symbol) for {atom:?}",
            );
            assert_eq!(
                sexp.as_keyword(),
                sexp.as_atom().and_then(Atom::as_keyword),
                "Sexp::as_keyword drifted from as_atom().and_then(Atom::as_keyword) for {atom:?}",
            );
            assert_eq!(
                sexp.as_string(),
                sexp.as_atom().and_then(Atom::as_string),
                "Sexp::as_string drifted from as_atom().and_then(Atom::as_string) for {atom:?}",
            );
            assert_eq!(
                sexp.as_int(),
                sexp.as_atom().and_then(Atom::as_int),
                "Sexp::as_int drifted from as_atom().and_then(Atom::as_int) for {atom:?}",
            );
            assert_eq!(
                sexp.as_bool(),
                sexp.as_atom().and_then(Atom::as_bool),
                "Sexp::as_bool drifted from as_atom().and_then(Atom::as_bool) for {atom:?}",
            );

            // `Sexp::as_float` specializes through the widening composition
            // `s.as_atom().and_then(|a| a.as_float().or_else(|| a.as_int()
            // .map(|n| n as f64)))` so the algebra-level `Atom::as_float`
            // stays strict and the typed-identity distinction `Int(1)` vs
            // `Float(1.0)` is preserved at the algebra layer.
            let expected_float = sexp
                .as_atom()
                .and_then(|a| a.as_float().or_else(|| a.as_int().map(|n| n as f64)));
            assert_eq!(
                sexp.as_float(),
                expected_float,
                "Sexp::as_float drifted from widening composition for {atom:?}",
            );
        }
    }

    #[test]
    fn sexp_as_float_widens_int_to_float_at_consumer_layer_only() {
        // CONSUMER-LAYER WIDENING CONTRACT: pin that the `Sexp::as_float`
        // consumer DOES widen `Atom::Int(n)` to `Some(n as f64)` (the
        // load-bearing widening at the numeric-kwarg boundary the
        // `extract_float` extractor depends on) AND that the algebra-
        // level `Atom::as_float` does NOT (the strict typed-identity
        // discipline pinned at `atom_as_float_returns_payload_iff_float_variant_strict_no_int_widening`).
        // The widening lives at the CONSUMER layer ONLY; a regression
        // that drifts the widening into the algebra layer (e.g. re-
        // adds an `Atom::Int(n) => Some(n as f64)` arm at
        // `Atom::as_float`) would silently coerce `Int(1)` slots into
        // the `Float` track at every `Atom` consumer that bypasses
        // `Sexp`, breaking the typed-identity discipline at the
        // canonical-form rendering surfaces (Display, JSON,
        // iac-forge).
        let int_sexp = Sexp::int(7);
        assert_eq!(
            int_sexp.as_float(),
            Some(7.0),
            "Sexp::as_float must widen Atom::Int to f64 at the consumer layer",
        );
        assert_eq!(
            Atom::Int(7).as_float(),
            None,
            "Atom::as_float must stay strict at the algebra layer",
        );

        // The widening sweeps the int domain — pin a few canonical
        // values so a regression that loses the `as f64` cast (e.g. an
        // accidental `usize` round-trip) surfaces directly.
        for n in [-42i64, -1, 0, 1, 42] {
            assert_eq!(
                Sexp::int(n).as_float(),
                Some(n as f64),
                "Sexp::as_float widening drifted for Int({n})",
            );
        }
    }

    #[test]
    fn sexp_to_json_method_projects_each_outer_arm_to_canonical_json() {
        // LIFTED-BOUNDARY CONTRACT: pin that the inherent
        // `Sexp::to_json()` method projects each reachable outer Sexp
        // shape to a `serde_json::Value` byte-identical to the
        // pre-lift inline rule at `crate::domain::sexp_to_json`'s
        // outer match — Nil → Null, Atom → `Atom::to_json` (composed
        // through the typed-algebra projection), List(kwargs) →
        // Object keyed by kebab→camel, List(other) → Array, and
        // each quote-family wrapper → recurse on inner (the wrapper
        // is structurally erased into JSON). A regression that
        // drifts ANY outer arm (e.g. emits Nil as `"nil"` instead of
        // Null, swaps List(kwargs) for Array unconditionally, drops
        // a quote-family arm's recursion) surfaces here. Pre-lift
        // the dispatcher lived as a free function in `domain.rs`;
        // post-lift the canonical site is the inherent method on
        // the `Sexp` algebra (same posture as the prior
        // `Sexp::shape` (121bb60) and `Sexp::witness` (a427e3b)
        // lifts).
        assert_eq!(
            Sexp::Nil.to_json().expect("nil to_json"),
            serde_json::Value::Null,
        );
        assert_eq!(
            Sexp::symbol("foo").to_json().expect("symbol to_json"),
            serde_json::Value::String("foo".into()),
        );
        assert_eq!(
            Sexp::keyword("k").to_json().expect("keyword to_json"),
            serde_json::Value::String(":k".into()),
        );
        assert_eq!(
            Sexp::string("body").to_json().expect("string to_json"),
            serde_json::Value::String("body".into()),
        );
        assert_eq!(
            Sexp::int(7).to_json().expect("int to_json"),
            serde_json::json!(7),
        );
        assert_eq!(
            Sexp::float(1.5).to_json().expect("float to_json"),
            serde_json::json!(1.5),
        );
        assert_eq!(
            Sexp::boolean(true).to_json().expect("true to_json"),
            serde_json::Value::Bool(true),
        );

        // List(kwargs) → Object with kebab→camel keys.
        let kwargs = Sexp::List(vec![
            Sexp::keyword("point-type"),
            Sexp::symbol("Gate"),
            Sexp::keyword("must-reach"),
            Sexp::boolean(true),
        ]);
        assert_eq!(
            kwargs.to_json().expect("kwargs list to_json"),
            serde_json::json!({"pointType": "Gate", "mustReach": true}),
        );

        // List(non-kwargs) → Array.
        let arr = Sexp::List(vec![Sexp::int(1), Sexp::int(2), Sexp::int(3)]);
        assert_eq!(
            arr.to_json().expect("non-kwargs list to_json"),
            serde_json::json!([1, 2, 3]),
        );

        // Empty list → Array (kwargs guard rejects empty lists).
        let empty = Sexp::List(vec![]);
        assert_eq!(
            empty.to_json().expect("empty list to_json"),
            serde_json::json!([]),
        );

        // Quote-family wrappers strip and recurse.
        let payload = Sexp::List(vec![Sexp::keyword("k"), Sexp::int(42)]);
        let expected = serde_json::json!({"k": 42});
        for wrapped in [
            Sexp::Quote(Box::new(payload.clone())),
            Sexp::Quasiquote(Box::new(payload.clone())),
            Sexp::Unquote(Box::new(payload.clone())),
            Sexp::UnquoteSplice(Box::new(payload.clone())),
        ] {
            assert_eq!(
                wrapped.to_json().expect("quote-family to_json"),
                expected,
                "quote-family wrapper {wrapped:?} drifted from inner-recursion shape",
            );
        }
    }

    #[test]
    fn sexp_to_json_method_agrees_with_domain_sexp_to_json_for_every_outer_shape() {
        // LIFTED-BOUNDARY CONTRACT: pin that the inherent
        // `Sexp::to_json()` method agrees with the free-function
        // delegate `crate::domain::sexp_to_json` for every reachable
        // outer shape. Pre-lift the dispatcher lived as a free
        // function in `domain.rs`; post-lift the canonical site is
        // the inherent method and the free function is a one-line
        // delegate. Pin that the delegation stays byte-for-byte
        // equivalent across every outer arm so a regression where
        // the free function drifts from the inherent method (or
        // vice versa) surfaces here immediately. Mirrors
        // `sexp_shape_method_agrees_with_domain_sexp_shape_for_every_outer_shape`
        // and
        // `sexp_witness_method_agrees_with_domain_sexp_witness_for_every_outer_shape`
        // for the JSON canonical-form projection peer.
        let samples = [
            Sexp::Nil,
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::int(-1),
            Sexp::float(7.5),
            Sexp::float(0.0),
            Sexp::boolean(true),
            Sexp::boolean(false),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::symbol("op"), Sexp::int(1), Sexp::int(2)]),
            Sexp::List(vec![
                Sexp::keyword("point-type"),
                Sexp::symbol("Gate"),
                Sexp::keyword("must-reach"),
                Sexp::boolean(true),
            ]),
            Sexp::Quote(Box::new(Sexp::symbol("payload"))),
            Sexp::Quasiquote(Box::new(Sexp::List(vec![Sexp::symbol("foo")]))),
            Sexp::Unquote(Box::new(Sexp::symbol("x"))),
            Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))),
        ];
        for s in &samples {
            let via_method = s.to_json().expect("method projection must succeed");
            let via_delegate =
                crate::domain::sexp_to_json(s).expect("delegate projection must succeed");
            assert_eq!(
                via_method, via_delegate,
                "Sexp::to_json drifted from domain::sexp_to_json at {s:?}",
            );
        }
    }

    #[test]
    fn sexp_to_json_method_routes_atom_arm_through_atom_to_json() {
        // PATH-UNIFORMITY CONTRACT: the lifted `Sexp::to_json()`
        // body composes through the typed-algebra primitive
        // [`Atom::to_json`] at the Atom arm — `Sexp::Atom(a).to_json()
        // == Ok(a.to_json())` for every atomic payload variant. A
        // regression in EITHER direction (a `Sexp::to_json` arm
        // that bypasses `Atom::to_json` and re-inlines a per-variant
        // mapping, or an `Atom::to_json` projection that diverges
        // from the rendering the outer arm depends on) is
        // structurally impossible — the typed JSON primitive composes
        // through the typed primitive halves once. Sibling-shape pin
        // to `sexp_to_json_atom_arms_route_through_atom_to_json` in
        // `domain.rs` (the free-function-delegate peer that pinned
        // the same identity at the pre-lift site).
        let cases: &[Atom] = &[
            Atom::Symbol("name".into()),
            Atom::Keyword("kw".into()),
            Atom::Str("body".into()),
            Atom::Int(7),
            Atom::Int(-3),
            Atom::Float(2.5),
            Atom::Float(1.0),
            Atom::Bool(true),
            Atom::Bool(false),
        ];
        for atom in cases {
            let via_method = Sexp::Atom(atom.clone())
                .to_json()
                .expect("atom must serialize through Sexp::to_json");
            let via_atom = atom.to_json();
            assert_eq!(
                via_method, via_atom,
                "Sexp::to_json Atom arm drifted from Atom::to_json for {atom:?}",
            );
        }
    }

    #[test]
    fn sexp_to_json_method_routes_quote_family_arms_through_inner_recursion() {
        // PATH-UNIFORMITY CONTRACT: the four quote-family arms each
        // strip the wrapper and recurse on the projected `inner`
        // (via `Self::expect_quote_form`), NOT on the outer `self`.
        // Pin that this binding semantic is observable across all
        // four wrappers: `wrap_qf(inner).to_json() == inner.to_json()`
        // for every `QuoteForm` variant. A regression that lifted
        // the recursion onto `self` (the outer wrapper) instead of
        // the projected inner would infinite-loop or surface as a
        // structural mismatch here. Sibling shape to
        // `sexp_to_json_routes_quote_family_arms_through_as_quote_form_typed_marker`
        // in `domain.rs::tests` (the free-function-delegate peer)
        // — both pin the same invariant at the lifted boundary.
        let inner = Sexp::List(vec![Sexp::keyword("k"), Sexp::int(42)]);
        let expected = inner.to_json().expect("inner serializes");
        for wrap in [
            Sexp::Quote(Box::new(inner.clone())),
            Sexp::Quasiquote(Box::new(inner.clone())),
            Sexp::Unquote(Box::new(inner.clone())),
            Sexp::UnquoteSplice(Box::new(inner.clone())),
        ] {
            let via_method = wrap
                .to_json()
                .expect("quote-family wrapper must serialize via Sexp::to_json");
            assert_eq!(
                via_method, expected,
                "Sexp::to_json drifted from inner-recursion shape at {wrap:?}",
            );
        }
    }

    #[test]
    fn sexp_to_json_method_rejects_duplicate_kwargs_at_lifted_boundary() {
        // TYPED-ENTRY CONTRACT: the duplicate-keyword rejection at
        // the kwargs-list arm fires at the inherent method directly,
        // not at the delegate — the canonical typed-entry gate lives
        // on the algebra. Pin that two `:k` entries in the same
        // kwargs list collapse to `LispError::DuplicateKwarg { key }`
        // with `key == "notify-ref"` (the kebab spelling, before
        // kebab→camel conversion — the diagnostic surface matches
        // the spelling the operator typed). The error type
        // discriminator is checked via debug-format substring so a
        // future LispError variant rename doesn't silently break
        // this pin. Mirrors `sexp_to_json_nested_duplicate_emits_structural_variant`
        // in `domain.rs::tests` (the free-function delegate peer at
        // the pre-lift site) at the lifted boundary.
        let dup = Sexp::List(vec![
            Sexp::keyword("notify-ref"),
            Sexp::string("a"),
            Sexp::keyword("notify-ref"),
            Sexp::string("b"),
        ]);
        let err = dup.to_json().expect_err("duplicate kwarg must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("DuplicateKwarg"),
            "expected DuplicateKwarg variant, got {rendered}",
        );
        assert!(
            rendered.contains("notify-ref"),
            "expected diagnostic to name the kebab-spelled duplicate key, got {rendered}",
        );
    }

    // ── Sexp::from_json: the inverse JSON-projection on the algebra ─────
    //
    // `Sexp::from_json` lifts the `domain::json_to_sexp` free-function
    // dispatcher onto the inherent-method canonical site on the [`Sexp`]
    // algebra — sibling-lift posture to the prior `sexp_to_json` →
    // `Sexp::to_json` (875ee3b), `sexp_witness` → `Sexp::witness`
    // (a427e3b), and `sexp_shape` → `Sexp::shape` (121bb60). The tests
    // below pin the per-arm contract on the new canonical site directly;
    // the free function delegates so the existing path-uniformity tests
    // at `domain::json_to_sexp_*` continue to pass post-lift unchanged.

    #[test]
    fn sexp_from_json_projects_each_outer_arm_to_canonical_sexp() {
        // LIFTED-BOUNDARY CONTRACT: pin that the inherent
        // `Sexp::from_json` associated function projects each reachable
        // outer `serde_json::Value` shape to a `Sexp` byte-identical to
        // the pre-lift inline rule at `crate::domain::json_to_sexp`'s
        // outer match — Null → Nil, Bool → boolean, Number(i64) → int,
        // Number(f64-only) → float, String → string, Array → List(map),
        // Object → List of alternating `:k v` pairs in iteration order
        // via `camel_to_kebab` on each key. A regression that drifts ANY
        // outer arm (e.g. emits Null as Sexp::string(""), swaps Array
        // for a kwargs-shaped List, drops the camel→kebab projection on
        // Object keys) surfaces here. Pre-lift the dispatcher lived as a
        // free function in `domain.rs`; post-lift the canonical site is
        // the inherent associated function on the `Sexp` algebra.
        assert_eq!(Sexp::from_json(&serde_json::Value::Null), Sexp::Nil);
        assert_eq!(
            Sexp::from_json(&serde_json::Value::Bool(true)),
            Sexp::boolean(true),
        );
        assert_eq!(
            Sexp::from_json(&serde_json::Value::Bool(false)),
            Sexp::boolean(false),
        );
        assert_eq!(Sexp::from_json(&serde_json::json!(42)), Sexp::int(42));
        assert_eq!(Sexp::from_json(&serde_json::json!(-1)), Sexp::int(-1));
        assert_eq!(Sexp::from_json(&serde_json::json!(0)), Sexp::int(0));
        // Float that does NOT fit i64 falls through to the float arm.
        assert_eq!(Sexp::from_json(&serde_json::json!(1.5)), Sexp::float(1.5));
        assert_eq!(
            Sexp::from_json(&serde_json::Value::String("body".into())),
            Sexp::string("body"),
        );

        // Array → List with each element projected recursively.
        let arr = serde_json::json!([1, "x", true, null]);
        assert_eq!(
            Sexp::from_json(&arr),
            Sexp::List(vec![
                Sexp::int(1),
                Sexp::string("x"),
                Sexp::boolean(true),
                Sexp::Nil,
            ]),
        );

        // Object → List of alternating `:k v` pairs, JSON key projected
        // through camel→kebab so the kwarg authoring shape is recovered.
        // The iteration order of the JSON object is implementation-
        // defined here (no `preserve_order` feature on `serde_json`), so
        // pin the SET of (kebab-key, value) pairs rather than the
        // sequence — order-uniformity vs. the delegate is pinned in the
        // path-uniformity test below.
        let obj = serde_json::json!({"pointType": "Gate", "mustReach": true});
        let result = Sexp::from_json(&obj);
        let items = match &result {
            Sexp::List(items) => items.clone(),
            other => panic!("expected List, got {other:?}"),
        };
        assert_eq!(items.len(), 4);
        let mut pairs: Vec<(String, Sexp)> = items
            .chunks_exact(2)
            .map(|c| (c[0].as_keyword().expect("kw").to_string(), c[1].clone()))
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            pairs,
            vec![
                ("must-reach".to_string(), Sexp::boolean(true)),
                ("point-type".to_string(), Sexp::string("Gate")),
            ],
        );
    }

    #[test]
    fn sexp_from_json_agrees_with_domain_json_to_sexp_for_every_outer_shape() {
        // PATH-UNIFORMITY GUARD: pin that the free-function delegate
        // `crate::domain::json_to_sexp(v) == Sexp::from_json(v)` for
        // every reachable `serde_json::Value` outer shape. Post-lift the
        // free function delegates to the inherent associated function;
        // this test pins the delegation byte-for-byte so a future
        // regression that drifts the delegate (e.g. inlines a stale
        // pre-lift body, swaps the iteration order at one site) fires
        // here, parallel to `sexp_to_json_method_agrees_with_domain_
        // sexp_to_json_for_every_outer_shape`'s posture for the forward
        // direction.
        let shapes = [
            serde_json::Value::Null,
            serde_json::Value::Bool(true),
            serde_json::Value::Bool(false),
            serde_json::json!(7),
            serde_json::json!(-3),
            serde_json::json!(2.5),
            serde_json::Value::String("body".into()),
            serde_json::json!([1, 2, 3]),
            serde_json::json!({"camelCase": "v", "another-key": 5}),
            serde_json::json!({"nested": {"inner": [1, 2]}}),
            serde_json::json!([]),
            serde_json::json!({}),
        ];
        for v in &shapes {
            assert_eq!(
                Sexp::from_json(v),
                crate::domain::json_to_sexp(v),
                "delegate drifted from inherent associated function for {v}",
            );
        }
    }

    #[test]
    fn sexp_from_json_object_keys_route_through_camel_to_kebab() {
        // KEY-PROJECTION CONTRACT: pin that JSON object keys land in
        // the resulting `Sexp::List` as `Sexp::keyword(camel_to_kebab(k))`
        // — the inverse of `Sexp::to_json`'s kebab→camel projection.
        // A regression that drops the projection (writes the JSON key
        // verbatim, breaking the kwarg round-trip), substitutes a
        // different camel→kebab implementation at this site, or routes
        // through `kebab_to_camel` (the wrong direction) surfaces here.
        let obj = serde_json::json!({
            "pointType": 1,
            "mustReach": 2,
            "already-kebab": 3,
            "withABC": 4,
        });
        let result = Sexp::from_json(&obj);
        let items = match &result {
            Sexp::List(items) => items,
            other => panic!("expected List, got {other:?}"),
        };
        // Even-position elements are keywords; odd-position elements are
        // values. Pin the keyword spellings against the camel→kebab
        // projection (camel boundaries become `-`; consecutive uppercase
        // each get a leading `-` per the implementation in
        // `domain::camel_to_kebab`).
        let kws: Vec<&str> = items
            .iter()
            .step_by(2)
            .map(|s| s.as_keyword().expect("even position must be keyword"))
            .collect();
        // Match the order JSON preserve_order gives us — sortable for
        // stability; the contract is just that each key landed through
        // camel→kebab, not the insertion order itself.
        let mut sorted = kws.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            vec!["already-kebab", "must-reach", "point-type", "with-a-b-c"],
        );
    }

    // ── Sexp::is_kwargs_list: the kwargs-shape predicate on the algebra ─
    //
    // `Sexp::is_kwargs_list` lifts the `pub(crate) domain::is_kwargs_list`
    // free function onto the inherent-method canonical site on the
    // [`Sexp`] algebra — sibling-shape predicate peer of [`Sexp::is_list`]
    // narrowing the structural witness to the kwargs-shaped sub-cohort.
    // The tests below pin the per-arm contract on the new canonical site
    // directly; the `pub(crate)` free function has zero remaining callers
    // post-lift and is removed in the same patch so the substrate's
    // "kwargs-shape predicate" lives at exactly one canonical site on the
    // algebra rather than splitting across a `domain.rs` helper and the
    // `Sexp::to_json` call site.

    #[test]
    fn sexp_is_kwargs_list_method_returns_true_for_canonical_kwargs_shape() {
        // PER-ARM CONTRACT (true cell): pin that a `Sexp::List` whose
        // even-indexed items are all keywords and whose length is non-zero
        // even returns `true` — the canonical kwargs shape `(:k v :k v …)`.
        // Covers the two-arity and four-arity baseline cases plus a mixed
        // payload (keyword odd index — even-index check is keyword-only,
        // odd-index payload is unconstrained per the kwargs convention).
        // A regression that drifts the predicate (incorrect parity check,
        // wrong keyword-position check, off-by-one in the step) surfaces
        // here immediately.
        let two = Sexp::List(vec![Sexp::keyword("k"), Sexp::int(1)]);
        assert!(two.is_kwargs_list());
        let four = Sexp::List(vec![
            Sexp::keyword("k1"),
            Sexp::int(1),
            Sexp::keyword("k2"),
            Sexp::string("v2"),
        ]);
        assert!(four.is_kwargs_list());
        // Odd-position values can themselves be keywords; the convention
        // only constrains the EVEN positions.
        let mixed = Sexp::List(vec![
            Sexp::keyword("k1"),
            Sexp::keyword("v-is-keyword-too"),
            Sexp::keyword("k2"),
            Sexp::Nil,
        ]);
        assert!(mixed.is_kwargs_list());
    }

    #[test]
    fn sexp_is_kwargs_list_method_returns_false_for_non_list_outer_shapes_and_violating_lists() {
        // PER-ARM CONTRACT (false cell): pin that every non-`Self::List`
        // outer shape (Nil, every Atom payload variant, every quote-family
        // wrapper) returns `false`, and that every `Self::List` violating
        // the kwargs convention (empty, odd length, non-keyword at any
        // even index) also returns `false`. A regression that returns
        // `true` for a wrong shape (e.g. claiming a Nil or a non-kwargs
        // list satisfies the predicate, opening the door to a
        // `Sexp::to_json` arm misrouting) surfaces here immediately.
        // Non-list outer shapes:
        assert!(!Sexp::Nil.is_kwargs_list());
        assert!(!Sexp::symbol("s").is_kwargs_list());
        assert!(!Sexp::keyword("k").is_kwargs_list());
        assert!(!Sexp::string("body").is_kwargs_list());
        assert!(!Sexp::int(0).is_kwargs_list());
        assert!(!Sexp::float(0.0).is_kwargs_list());
        assert!(!Sexp::boolean(true).is_kwargs_list());
        assert!(!Sexp::Quote(Box::new(Sexp::keyword("k"))).is_kwargs_list());
        assert!(!Sexp::Quasiquote(Box::new(Sexp::keyword("k"))).is_kwargs_list());
        assert!(!Sexp::Unquote(Box::new(Sexp::keyword("k"))).is_kwargs_list());
        assert!(!Sexp::UnquoteSplice(Box::new(Sexp::keyword("k"))).is_kwargs_list());
        // List arm violations:
        assert!(!Sexp::List(vec![]).is_kwargs_list()); // empty
        assert!(!Sexp::List(vec![Sexp::keyword("k")]).is_kwargs_list()); // odd length 1
        assert!(
            !Sexp::List(vec![Sexp::keyword("k1"), Sexp::int(1), Sexp::keyword("k2")])
                .is_kwargs_list()
        ); // odd length 3
        assert!(!Sexp::List(vec![Sexp::int(1), Sexp::int(2)]).is_kwargs_list()); // non-keyword at even 0
        assert!(!Sexp::List(vec![
            Sexp::keyword("k1"),
            Sexp::int(1),
            Sexp::symbol("not-kw"),
            Sexp::int(2)
        ])
        .is_kwargs_list()); // non-keyword at even 2
    }

    #[test]
    fn sexp_is_kwargs_list_method_composes_through_as_list_and_atom_as_keyword() {
        // COMPOSITION LAW: pin that the lifted predicate composes through
        // the already-lifted `Self::as_list` (structural projection onto
        // `&[Sexp]`) and `Atom::as_keyword` (typed projection onto the
        // keyword payload) primitives — a regression that re-inlines the
        // body without routing through the algebra-level soft-projection
        // family becomes detectable here. Sweeps every reachable outer
        // shape (Nil, every Atom variant, every quote-family wrapper, a
        // selection of List shapes covering the true + false cells) and
        // asserts the predicate's value agrees with the by-hand
        // `as_list().is_some_and(...)` recomposition.
        fn by_hand(s: &Sexp) -> bool {
            s.as_list().is_some_and(|items| {
                !items.is_empty()
                    && items.len().is_multiple_of(2)
                    && items.iter().step_by(2).all(|e| e.as_keyword().is_some())
            })
        }
        let cases = [
            Sexp::Nil,
            Sexp::symbol("s"),
            Sexp::keyword("k"),
            Sexp::string("body"),
            Sexp::int(7),
            Sexp::float(2.5),
            Sexp::boolean(false),
            Sexp::Quote(Box::new(Sexp::keyword("k"))),
            Sexp::Quasiquote(Box::new(Sexp::keyword("k"))),
            Sexp::Unquote(Box::new(Sexp::keyword("k"))),
            Sexp::UnquoteSplice(Box::new(Sexp::keyword("k"))),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(1)]),
            Sexp::List(vec![Sexp::keyword("k"), Sexp::int(1)]),
            Sexp::List(vec![
                Sexp::keyword("k1"),
                Sexp::int(1),
                Sexp::keyword("k2"),
                Sexp::int(2),
            ]),
            Sexp::List(vec![Sexp::int(1), Sexp::int(2)]),
            Sexp::List(vec![Sexp::keyword("k1"), Sexp::int(1), Sexp::symbol("x")]),
        ];
        for s in &cases {
            assert_eq!(
                s.is_kwargs_list(),
                by_hand(s),
                "predicate drifted from as_list ∘ atom_as_keyword composition for {s}",
            );
        }
    }

    #[test]
    fn sexp_to_json_object_arm_routes_through_is_kwargs_list_method() {
        // CALLSITE-CONTRACT: pin that `Sexp::to_json`'s kwargs-vs-array
        // bifurcation routes through the lifted `Sexp::is_kwargs_list`
        // method — the kwargs-shape witness that gates the
        // `serde_json::Value::Object` arm vs the `serde_json::Value::Array`
        // arm at the `Sexp::List` outer shape. The pin walks the gate
        // both directions: a kwargs-shaped list must project as `Object`
        // (and the inherent predicate must agree, `true`); a non-kwargs
        // list (empty, odd-length, or even-index non-keyword) must
        // project as `Array` (and the predicate must agree, `false`). A
        // regression that decouples the two paths (e.g. `to_json` routes
        // through a re-inlined check while `is_kwargs_list` continues to
        // delegate, or vice versa) surfaces here.
        // Kwargs-shaped: Object projection, predicate true.
        let kw = Sexp::List(vec![Sexp::keyword("foo-bar"), Sexp::int(1)]);
        assert!(kw.is_kwargs_list());
        assert!(matches!(
            kw.to_json().expect("kwargs list projects"),
            serde_json::Value::Object(_)
        ));
        // Non-kwargs (empty list): Array projection, predicate false.
        let empty = Sexp::List(vec![]);
        assert!(!empty.is_kwargs_list());
        assert!(matches!(
            empty.to_json().expect("empty list projects"),
            serde_json::Value::Array(arr) if arr.is_empty(),
        ));
        // Non-kwargs (positional): Array projection, predicate false.
        let positional = Sexp::List(vec![Sexp::int(1), Sexp::int(2), Sexp::int(3)]);
        assert!(!positional.is_kwargs_list());
        assert!(matches!(
            positional.to_json().expect("positional list projects"),
            serde_json::Value::Array(arr) if arr.len() == 3,
        ));
        // Non-kwargs (even-index non-keyword): Array projection.
        let mixed = Sexp::List(vec![
            Sexp::keyword("k"),
            Sexp::int(1),
            Sexp::symbol("x"),
            Sexp::int(2),
        ]);
        assert!(!mixed.is_kwargs_list());
        assert!(matches!(
            mixed.to_json().expect("mixed list projects"),
            serde_json::Value::Array(_)
        ));
    }

    #[test]
    fn sexp_from_json_round_trips_to_json_for_canonical_subset() {
        // ROUND-TRIP LAW: pin `Sexp::to_json(s)?.from_json() == s` for
        // the round-trippable subset of Sexp shapes — Nil, Atom::Str
        // (the lossless atomic floor that absorbs Symbol/Keyword on
        // re-projection, so this test stays inside the lossless cell),
        // Atom::Int, Atom::Float, Atom::Bool, and recursively
        // Sexp::List of round-trippable elements. Pin that the inverse
        // composes byte-for-byte against the forward projection inside
        // the lossless cell — the round-trip law's structural anchor
        // documented at `Sexp::from_json`'s docstring.
        let cases = [
            Sexp::Nil,
            Sexp::string("body"),
            Sexp::int(42),
            Sexp::float(1.5),
            Sexp::boolean(true),
            Sexp::List(vec![Sexp::int(1), Sexp::string("x"), Sexp::Nil]),
            // Empty list → empty array → empty list. Round-trips cleanly.
            Sexp::List(vec![]),
        ];
        for s in &cases {
            let projected = s
                .to_json()
                .expect("round-trippable Sexp must project to JSON");
            let recovered = Sexp::from_json(&projected);
            assert_eq!(recovered, *s, "round-trip drifted at {s}");
        }
    }

    // ── Atom typed-construct family + Sexp outer-constructor routing ─────
    //
    // The six `Atom::{symbol, keyword, string, int, float, boolean}`
    // typed-construct methods are the section sibling of the existing
    // six `Atom::as_{symbol, keyword, string, int, float, bool}` soft-
    // projection family — closing the (construct, project) algebra dual
    // on the closed-set `Atom` algebra. The six `Sexp::{symbol, ...,
    // boolean}` outer constructors now route through
    // `Self::Atom(Atom::X(_))` so the `impl Into<String>` ergonomy +
    // tuple-variant constructor pair lives at ONE site per kind on the
    // `Atom` algebra. Pin the four structural laws:
    //   (a) each `Atom::X` constructor produces the canonical tuple
    //       variant payload byte-for-byte (`Atom::symbol("foo") ==
    //       Atom::Symbol("foo".into())`, etc.) — pre-lift behavior
    //       under the new construction face;
    //   (b) the (construct, kind-project) round-trip
    //       `Atom::X(_).kind() == AtomKind::X` for every (kind, payload)
    //       pair — the typed-construct family pairs section-for-
    //       retraction with the `Atom::kind` projection;
    //   (c) the (construct, soft-project) round-trip
    //       `Atom::X(payload).as_X() == Some(payload)` for every kind —
    //       the typed-construct family pairs section-for-retraction
    //       with the `Atom::as_X` family it now siblings;
    //   (d) the outer-constructor composition law `Sexp::X(p) ==
    //       Sexp::Atom(Atom::X(p))` for every kind — the `Sexp` outer
    //       constructors route through the typed `Atom` constructors
    //       rather than re-deriving the `Self::Atom(Atom::X(_))` pair
    //       inline.

    #[test]
    fn atom_typed_constructors_emit_canonical_tuple_variant_for_every_kind() {
        // STRUCTURAL CONSTRUCT CONTRACT: each `Atom::X` constructor
        // emits the matching `Atom::Variant(payload)` tuple-variant
        // value byte-for-byte. A regression that drifts ONE arm (e.g.
        // a typo routing `Atom::keyword(s)` to `Self::Symbol(s.into())`
        // — type-checks but silently mis-classifies every kwarg key
        // authored through the algebra-level constructor) surfaces
        // here. The `impl Into<String>` arms also accept `String`
        // payloads — pinned alongside `&str` so the `.into()` ergonomy
        // is exercised across both source types.
        assert_eq!(Atom::symbol("foo"), Atom::Symbol("foo".into()));
        assert_eq!(
            Atom::symbol(String::from("seph.1")),
            Atom::Symbol("seph.1".into()),
        );
        assert_eq!(Atom::symbol(""), Atom::Symbol(String::new()));
        assert_eq!(Atom::keyword("parent"), Atom::Keyword("parent".into()));
        assert_eq!(
            Atom::keyword(String::from("attr")),
            Atom::Keyword("attr".into()),
        );
        assert_eq!(Atom::keyword(""), Atom::Keyword(String::new()));
        assert_eq!(Atom::string("body"), Atom::Str("body".into()));
        assert_eq!(
            Atom::string(String::from("with\nnewline")),
            Atom::Str("with\nnewline".into()),
        );
        assert_eq!(Atom::string(""), Atom::Str(String::new()));
        assert_eq!(Atom::int(0), Atom::Int(0));
        assert_eq!(Atom::int(42), Atom::Int(42));
        assert_eq!(Atom::int(-7), Atom::Int(-7));
        assert_eq!(Atom::int(i64::MIN), Atom::Int(i64::MIN));
        assert_eq!(Atom::int(i64::MAX), Atom::Int(i64::MAX));
        assert_eq!(Atom::float(0.0), Atom::Float(0.0));
        assert_eq!(Atom::float(1.5), Atom::Float(1.5));
        assert_eq!(Atom::float(-2.5), Atom::Float(-2.5));
        // NaN compares unequal to itself; pin via `to_bits` round-trip,
        // matching the `Hash for Atom` Float-arm posture
        // (`f.to_bits().hash(...)`).
        assert_eq!(Atom::float(f64::NAN).kind(), AtomKind::Float);
        match Atom::float(f64::NAN) {
            Atom::Float(n) => assert!(n.is_nan()),
            _ => panic!("Atom::float must emit Atom::Float"),
        }
        assert_eq!(Atom::float(f64::INFINITY), Atom::Float(f64::INFINITY));
        assert_eq!(Atom::boolean(true), Atom::Bool(true));
        assert_eq!(Atom::boolean(false), Atom::Bool(false));
    }

    #[test]
    fn atom_typed_constructors_round_trip_through_kind_projection() {
        // SECTION LAW (construct → kind): every typed constructor's
        // output projects through `Atom::kind` to its matching
        // `AtomKind` variant. The `(construct, kind-project)` pair
        // forms a deterministic surjection from the construct face
        // onto the closed-set `AtomKind` algebra — six (kind,
        // representative payload) probes sweep `AtomKind::ALL` so a
        // future seventh atomic kind landing on the algebra extends
        // BOTH the construct face AND this sweep in lockstep (rustc-
        // enforced through the closed-set match below).
        for kind in AtomKind::ALL {
            let constructed = match kind {
                AtomKind::Symbol => Atom::symbol("foo"),
                AtomKind::Keyword => Atom::keyword("parent"),
                AtomKind::Str => Atom::string("body"),
                AtomKind::Int => Atom::int(42),
                AtomKind::Float => Atom::float(1.5),
                AtomKind::Bool => Atom::boolean(true),
            };
            assert_eq!(
                constructed.kind(),
                kind,
                "Atom typed constructor for {kind:?} drifted from its closed-set kind projection",
            );
        }
    }

    #[test]
    fn atom_typed_constructors_round_trip_through_per_variant_soft_projection() {
        // RETRACTION LAW (construct → soft-project): every typed
        // constructor's output projects through its matching `Atom::as_X`
        // soft projection to `Some(payload)` — the (construct, soft-
        // project) pair forms an `Iso(payload, Atom::Variant(payload))`
        // on the typed-payload axis. Sibling-axis to the
        // `(construct, kind-project)` pair above and to the
        // `Sexp::as_quote_form / QuoteForm::wrap` round-trip on the
        // outer-shape axis (`QuoteForm::wrap(inner).as_quote_form()
        // == Some((qf, &inner))`). The retraction's load-bearing
        // contract is what the substrate's named-form NAME gate
        // (`split_name_slot` → `as_symbol_or_string`) depends on at
        // every typed-domain dispatcher.
        assert_eq!(Atom::symbol("foo").as_symbol(), Some("foo"));
        assert_eq!(Atom::symbol("").as_symbol(), Some(""));
        assert_eq!(Atom::keyword("parent").as_keyword(), Some("parent"));
        assert_eq!(Atom::keyword("").as_keyword(), Some(""));
        assert_eq!(Atom::string("body").as_string(), Some("body"));
        assert_eq!(Atom::string("").as_string(), Some(""));
        assert_eq!(Atom::int(42).as_int(), Some(42));
        assert_eq!(Atom::int(0).as_int(), Some(0));
        assert_eq!(Atom::int(i64::MIN).as_int(), Some(i64::MIN));
        assert_eq!(Atom::float(1.5).as_float(), Some(1.5));
        assert_eq!(Atom::float(0.0).as_float(), Some(0.0));
        assert_eq!(Atom::boolean(true).as_bool(), Some(true));
        assert_eq!(Atom::boolean(false).as_bool(), Some(false));
    }

    #[test]
    fn sexp_outer_constructors_route_through_atom_typed_construct_family() {
        // OUTER-CONSTRUCTOR COMPOSITION LAW: pin that each `Sexp::X`
        // outer constructor emits `Sexp::Atom(Atom::X(_))` byte-for-byte
        // — a regression that re-inlines the pre-lift body
        // `Self::Atom(Atom::Variant(s.into()))` and drifts ONE arm
        // (e.g. a future copy-edit that swaps `Sexp::symbol` to route
        // through `Atom::Keyword` after a refactor) becomes detectable
        // at this site. Sibling-shape pin to the `Sexp::as_X` family's
        // structural-lift composition through `Sexp::as_atom +
        // Atom::as_X` on the projection axis (sweep posture in
        // `sexp_as_symbol_or_string_routes_through_atom_as_symbol_or_string_via_as_atom_composition`).
        assert_eq!(Sexp::symbol("foo"), Sexp::Atom(Atom::symbol("foo")));
        assert_eq!(Sexp::symbol(""), Sexp::Atom(Atom::symbol("")));
        assert_eq!(
            Sexp::symbol(String::from("seph.1")),
            Sexp::Atom(Atom::symbol("seph.1")),
        );
        assert_eq!(Sexp::keyword("parent"), Sexp::Atom(Atom::keyword("parent")),);
        assert_eq!(Sexp::string("body"), Sexp::Atom(Atom::string("body")));
        assert_eq!(Sexp::int(42), Sexp::Atom(Atom::int(42)));
        assert_eq!(Sexp::int(i64::MIN), Sexp::Atom(Atom::int(i64::MIN)));
        assert_eq!(Sexp::float(1.5), Sexp::Atom(Atom::float(1.5)));
        assert_eq!(Sexp::boolean(true), Sexp::Atom(Atom::boolean(true)));
        assert_eq!(Sexp::boolean(false), Sexp::Atom(Atom::boolean(false)));
    }

    #[test]
    fn atom_typed_constructors_partition_atom_kind_across_constructed_payloads() {
        // PARTITION LAW: every typed constructor's output projects to
        // `Some(_)` on its matching soft projection AND to `None` on
        // every other soft projection. The (construct, soft-project)
        // matrix is the diagonal of `AtomKind::ALL × AtomKind::ALL`:
        // on-diagonal cells return `Some`, off-diagonal cells return
        // `None`. Pin the full matrix so a regression that conflates
        // two construct arms (e.g. a future `Atom::keyword(s)` typo
        // routing to `Self::Symbol(s.into())` — type-checks, passes
        // the kind-projection sweep above iff the typo also drifts
        // `Atom::kind`, but fails THIS sweep because the off-diagonal
        // `Atom::keyword(s).as_symbol() == None` cell flips to `Some`)
        // surfaces structurally. The matrix's diagonal-restriction
        // form rebuilds the closed-set partition law every soft-
        // projection sweep above pins per-axis into ONE joint pin
        // across the (construct, project) algebra dual.
        let constructed = [
            (AtomKind::Symbol, Atom::symbol("foo")),
            (AtomKind::Keyword, Atom::keyword("parent")),
            (AtomKind::Str, Atom::string("body")),
            (AtomKind::Int, Atom::int(42)),
            (AtomKind::Float, Atom::float(1.5)),
            (AtomKind::Bool, Atom::boolean(true)),
        ];
        for (built_kind, a) in &constructed {
            assert_eq!(
                a.as_symbol().is_some(),
                *built_kind == AtomKind::Symbol,
                "as_symbol partition row drifted for {built_kind:?}",
            );
            assert_eq!(
                a.as_keyword().is_some(),
                *built_kind == AtomKind::Keyword,
                "as_keyword partition row drifted for {built_kind:?}",
            );
            assert_eq!(
                a.as_string().is_some(),
                *built_kind == AtomKind::Str,
                "as_string partition row drifted for {built_kind:?}",
            );
            assert_eq!(
                a.as_int().is_some(),
                *built_kind == AtomKind::Int,
                "as_int partition row drifted for {built_kind:?}",
            );
            assert_eq!(
                a.as_float().is_some(),
                *built_kind == AtomKind::Float,
                "as_float partition row drifted for {built_kind:?}",
            );
            assert_eq!(
                a.as_bool().is_some(),
                *built_kind == AtomKind::Bool,
                "as_bool partition row drifted for {built_kind:?}",
            );
        }
    }

    // ── Sexp quote-family typed-construct algebra ────────────────────────
    //
    // `Sexp::quote` / `Sexp::quasiquote` / `Sexp::unquote` /
    // `Sexp::unquote_splice` are the outer-Sexp typed-construct family for
    // the four homoiconic prefix wrappers, section-for-retraction with the
    // `Sexp::as_quote_form` soft-projection sibling. Each routes through
    // `QuoteForm::X.wrap(inner)` so the (marker, `Sexp::* tuple-variant
    // constructor + `Box::new`) welded triple lives at ONE site on the
    // closed-set `QuoteForm` algebra. Pin FOUR structural laws:
    //   (a) the canonical-tuple emission
    //       `Sexp::quote(inner) == Sexp::Quote(Box::new(inner))` for
    //       every wrapper marker — the typed constructor pairs section-
    //       for-retraction with the tuple-variant constructor;
    //   (b) the composition law
    //       `Sexp::X_variant(inner) == QuoteForm::X.wrap(inner)` for
    //       every marker — the outer typed constructor routes through
    //       the inner-algebra `QuoteForm::wrap` typed dispatch;
    //   (c) the round-trip law
    //       `Sexp::X_variant(inner).as_quote_form() == Some((QuoteForm::X,
    //       &inner))` for every marker — the (construct, soft-project)
    //       algebra dual closes on the outer [`Sexp`] algebra with
    //       marker + inner-body cross-projection preserved;
    //   (d) the outer-shape pairing
    //       `Sexp::X_variant(inner).shape() == QuoteForm::X.sexp_shape()`
    //       for every marker — the construct family composes coherently
    //       through the outer-shape projection on the typed-shape
    //       lattice, so a regression that drifts ONE marker's outer-
    //       shape pairing from `QuoteForm::sexp_shape` surfaces here.

    #[test]
    fn sexp_quote_family_constructors_emit_canonical_tuple_variant_for_every_marker() {
        // STRUCTURAL CONSTRUCT CONTRACT: each `Sexp::X_variant`
        // constructor emits the matching `Sexp::X(Box::new(inner))`
        // tuple-variant value byte-for-byte. A regression that drifts
        // ONE arm (e.g. a typo routing `Sexp::unquote(inner)` to
        // `Sexp::UnquoteSplice(Box::new(inner))` — type-checks but
        // silently mis-classifies every macro-template substitution
        // authored through the algebra-level constructor) surfaces
        // here. Sibling-shape pin to the `Atom` typed-construct
        // family's canonical-tuple-variant test posture
        // (`atom_typed_constructors_emit_canonical_tuple_variant_for_every_kind`).
        let payloads = [
            Sexp::Nil,
            Sexp::symbol("x"),
            Sexp::keyword("k"),
            Sexp::string("body"),
            Sexp::int(42),
            Sexp::boolean(true),
            Sexp::List(vec![Sexp::symbol("op"), Sexp::int(1)]),
        ];
        for inner in &payloads {
            assert_eq!(
                Sexp::quote(inner.clone()),
                Sexp::Quote(Box::new(inner.clone())),
                "Sexp::quote drifted from canonical tuple variant for {inner:?}",
            );
            assert_eq!(
                Sexp::quasiquote(inner.clone()),
                Sexp::Quasiquote(Box::new(inner.clone())),
                "Sexp::quasiquote drifted from canonical tuple variant for {inner:?}",
            );
            assert_eq!(
                Sexp::unquote(inner.clone()),
                Sexp::Unquote(Box::new(inner.clone())),
                "Sexp::unquote drifted from canonical tuple variant for {inner:?}",
            );
            assert_eq!(
                Sexp::unquote_splice(inner.clone()),
                Sexp::UnquoteSplice(Box::new(inner.clone())),
                "Sexp::unquote_splice drifted from canonical tuple variant for {inner:?}",
            );
        }
    }

    #[test]
    fn sexp_quote_family_constructors_route_through_quote_form_wrap() {
        // COMPOSITION LAW: pin that each `Sexp::X_variant` outer
        // constructor emits `QuoteForm::X.wrap(inner)` byte-for-byte —
        // a regression that re-inlines the pre-lift body
        // `Self::X(Box::new(inner))` and drifts ONE arm (e.g. a future
        // copy-edit that swaps `Sexp::quote` to route through
        // `QuoteForm::Quasiquote` after a refactor) becomes detectable
        // at this site. Sibling-shape pin to the `Sexp::X_atom` family's
        // composition-through-`Atom::X` posture
        // (`sexp_outer_constructors_route_through_atom_typed_construct_family`).
        let inner = Sexp::List(vec![Sexp::symbol("op"), Sexp::int(1)]);
        assert_eq!(
            Sexp::quote(inner.clone()),
            QuoteForm::Quote.wrap(inner.clone())
        );
        assert_eq!(
            Sexp::quasiquote(inner.clone()),
            QuoteForm::Quasiquote.wrap(inner.clone()),
        );
        assert_eq!(
            Sexp::unquote(inner.clone()),
            QuoteForm::Unquote.wrap(inner.clone())
        );
        assert_eq!(
            Sexp::unquote_splice(inner.clone()),
            QuoteForm::UnquoteSplice.wrap(inner.clone()),
        );
    }

    #[test]
    fn sexp_quote_family_constructors_round_trip_through_as_quote_form() {
        // ROUND-TRIP LAW (construct → soft-project): every quote-family
        // typed constructor's output projects through `Sexp::as_quote_form`
        // to `Some((matching QuoteForm, &inner))`. Sweeps `QuoteForm::ALL`
        // paired with a representative inner payload — the four
        // (construct, project) pairs form an `Iso(inner, Sexp::X(inner))`
        // on the typed-marker axis at the outer [`Sexp`] algebra. A
        // regression that drifts ONE marker's construct arm (marker/
        // constructor swap) fails BOTH the marker-projection AND the
        // inner-borrow round-trip. Sibling-shape pin to the `Atom` typed-
        // construct family's per-variant soft-projection round-trip test
        // posture
        // (`atom_typed_constructors_round_trip_through_per_variant_soft_projection`).
        let inner = Sexp::List(vec![Sexp::symbol("op"), Sexp::int(1)]);
        let constructed: [(QuoteForm, Sexp); 4] = [
            (QuoteForm::Quote, Sexp::quote(inner.clone())),
            (QuoteForm::Quasiquote, Sexp::quasiquote(inner.clone())),
            (QuoteForm::Unquote, Sexp::unquote(inner.clone())),
            (
                QuoteForm::UnquoteSplice,
                Sexp::unquote_splice(inner.clone()),
            ),
        ];
        for qf in QuoteForm::ALL {
            let (built_qf, sexp) = constructed
                .iter()
                .find(|(m, _)| *m == qf)
                .expect("QuoteForm::ALL sweep must reach every marker");
            assert_eq!(*built_qf, qf);
            let (proj_qf, proj_inner) = sexp
                .as_quote_form()
                .unwrap_or_else(|| panic!("construct→as_quote_form drifted at {qf:?}"));
            assert_eq!(
                proj_qf, qf,
                "typed-marker round-trip drifted at {qf:?} — construct+project pair broken",
            );
            assert_eq!(
                proj_inner, &inner,
                "inner-body round-trip drifted at {qf:?} — construct+project pair broken",
            );
        }
    }

    #[test]
    fn sexp_quote_family_constructors_compose_with_shape_via_quote_form_sexp_shape() {
        // OUTER-SHAPE COMPOSITION LAW: every quote-family typed
        // constructor's output projects through `Sexp::shape` to the
        // matching `QuoteForm::X.sexp_shape()` — the (construct,
        // outer-shape) composition binds through the closed-set
        // `QuoteForm::sexp_shape` embed already lifted onto the
        // typed-shape lattice. A regression that drifts ONE construct
        // arm's outer-shape from `QuoteForm::sexp_shape` (e.g. a future
        // marker/wrapper swap that surfaces through the typed-shape
        // lattice but not through the tuple-variant emission itself)
        // surfaces here alongside the round-trip pin. Sibling-shape pin
        // to `quote_form_sexp_shape_paired_with_as_quote_form_preserves_pre_lift_pairing_for_every_sexp`
        // on the projection axis — this pin closes the same axis on the
        // outer construct family.
        let inner = Sexp::List(vec![Sexp::symbol("op"), Sexp::int(1)]);
        let constructed: [(QuoteForm, Sexp); 4] = [
            (QuoteForm::Quote, Sexp::quote(inner.clone())),
            (QuoteForm::Quasiquote, Sexp::quasiquote(inner.clone())),
            (QuoteForm::Unquote, Sexp::unquote(inner.clone())),
            (
                QuoteForm::UnquoteSplice,
                Sexp::unquote_splice(inner.clone()),
            ),
        ];
        for (qf, sexp) in &constructed {
            assert_eq!(
                sexp.shape(),
                qf.sexp_shape(),
                "Sexp::X_variant→shape drifted from QuoteForm::sexp_shape at {qf:?}",
            );
        }
    }

    // ── Sexp::list residual-axis typed-construct algebra ─────────────────
    //
    // `Sexp::list(items)` is the residual-axis section-for-retraction
    // sibling of the pre-existing `Sexp::as_list` soft-projection — the
    // (construct, project) algebra dual on the 2-of-12 residual carving of
    // the [`SexpShape`] closed set now closes at ONE constructor + ONE
    // projection on the outer [`Sexp`] algebra, symmetric with the atomic-
    // payload carving's (six `Sexp::X_atom(payload)` constructors +
    // `Sexp::as_atom` / `Sexp::as_atom_kind` projections) and the quote-
    // family carving's (four `Sexp::X_variant(inner)` constructors +
    // `Sexp::as_quote_form` / `Sexp::as_quote_form_marker` projections).
    // [`Sexp::Nil`] is a unit variant with no payload — the residual-axis
    // construct family closes at ONE constructor (the sole payload-bearing
    // residual arm). Pin FIVE structural laws:
    //   (a) the canonical-tuple emission
    //       `Sexp::list(items) == Sexp::List(items.into_iter().collect())`
    //       across representative empty / single-element / multi-element /
    //       heterogeneous-inner samples — the typed constructor pairs
    //       section-for-retraction with the tuple-variant constructor;
    //   (b) the round-trip law
    //       `Sexp::list(items.clone()).as_list() == Some(items.as_slice())`
    //       — the (construct, soft-project) algebra dual closes on the
    //       outer [`Sexp`] algebra with the borrowed-slice cross-
    //       projection preserving identity;
    //   (c) the outer-shape law
    //       `Sexp::list(items).shape() == SexpShape::List` — the residual-
    //       arm outer-shape identity binds through the typed-shape
    //       lattice at ONE arm, symmetric with the quote-family
    //       construct family's `Sexp::X_variant(inner).shape() ==
    //       QuoteForm::X.sexp_shape()`;
    //   (d) the structural-kind law
    //       `Sexp::list(items).as_structural_kind() == Some(
    //       StructuralKind::List)` — the residual carving marker binds
    //       through the closed-set [`StructuralKind`] algebra at ONE
    //       arm, symmetric with the atomic-axis's
    //       `Sexp::X_atom(payload).as_atom_kind() == Some(AtomKind::X)`;
    //   (e) the input-shape flexibility
    //       `Sexp::list(&Vec<Sexp>)` / `Sexp::list([Sexp; N])` /
    //       `Sexp::list(iter::map(...))` all agree with the canonical
    //       tuple-variant emission — the `impl IntoIterator<Item = Sexp>`
    //       bound accepts every reasonable owned-sequence shape without a
    //       per-consumer `.collect::<Vec<Sexp>>()` coercion.

    #[test]
    fn sexp_list_constructor_emits_canonical_tuple_variant_across_representative_inputs() {
        // STRUCTURAL CONSTRUCT CONTRACT: `Sexp::list(items)` emits
        // `Sexp::List(items.into_iter().collect::<Vec<Sexp>>())` byte-
        // for-byte across representative empty, single-element, multi-
        // element, and heterogeneous-inner samples. A regression that
        // drifts the body (e.g. wrapping items in an extra `Sexp::Nil`
        // sentinel, deduplicating, filtering) surfaces here. Sibling-
        // shape pin to the quote-family construct family's canonical-
        // tuple-variant test posture
        // (`sexp_quote_family_constructors_emit_canonical_tuple_variant_for_every_marker`).
        let samples: [Vec<Sexp>; 5] = [
            vec![],
            vec![Sexp::symbol("only")],
            vec![Sexp::symbol("op"), Sexp::int(1), Sexp::int(2)],
            vec![
                Sexp::Nil,
                Sexp::keyword("k"),
                Sexp::string("body"),
                Sexp::boolean(true),
                Sexp::List(vec![Sexp::symbol("nested")]),
            ],
            vec![
                Sexp::Quote(Box::new(Sexp::symbol("x"))),
                Sexp::Quasiquote(Box::new(Sexp::List(vec![
                    Sexp::symbol("template"),
                    Sexp::Unquote(Box::new(Sexp::symbol("var"))),
                ]))),
                Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))),
            ],
        ];
        for items in &samples {
            assert_eq!(
                Sexp::list(items.clone()),
                Sexp::List(items.clone()),
                "Sexp::list drifted from canonical Sexp::List(_) tuple variant for {items:?}",
            );
        }
    }

    #[test]
    fn sexp_list_constructor_round_trips_through_as_list() {
        // ROUND-TRIP LAW (section-for-retraction on the residual axis):
        // `Sexp::list(items.clone()).as_list() == Some(items.as_slice())`
        // sweeps the same representative input matrix as the canonical-
        // tuple pin — proves the (construct, soft-project) pair forms an
        // `Iso(Vec<Sexp>, Sexp::List(Vec<Sexp>))` on the residual axis,
        // symmetric with the quote-family axis's `Sexp::X_variant(inner)
        // .as_quote_form() == Some((QuoteForm::X, &inner))` round-trip
        // (pinned by `sexp_quote_family_constructors_round_trip_through_as_quote_form`).
        // A regression that mis-implements `Sexp::list` (e.g. dropping
        // items, cloning off-by-one) fails here on top of the canonical-
        // tuple pin.
        let samples: [Vec<Sexp>; 4] = [
            vec![],
            vec![Sexp::symbol("solo")],
            vec![Sexp::symbol("op"), Sexp::int(1), Sexp::int(2)],
            vec![
                Sexp::Nil,
                Sexp::List(vec![Sexp::symbol("nested"), Sexp::int(7)]),
                Sexp::Quote(Box::new(Sexp::symbol("q"))),
            ],
        ];
        for items in &samples {
            let built = Sexp::list(items.clone());
            assert_eq!(
                built.as_list(),
                Some(items.as_slice()),
                "Sexp::list→as_list round-trip drifted for {items:?}",
            );
        }
    }

    #[test]
    fn sexp_list_constructor_composes_with_shape_via_sexp_shape_list() {
        // OUTER-SHAPE COMPOSITION LAW: every `Sexp::list(items)` output
        // projects through `Sexp::shape` to `SexpShape::List` regardless
        // of inner-item content — the (construct, outer-shape)
        // composition binds through the typed-shape lattice's residual-
        // arm at ONE arm. Sibling-shape pin to the quote-family construct
        // family's outer-shape composition
        // (`sexp_quote_family_constructors_compose_with_shape_via_quote_form_sexp_shape`).
        // A regression that reroutes `Sexp::list` through another shape
        // arm (e.g. wrapping in `Sexp::Quote` after a copy-edit that
        // type-checks) surfaces here alongside the canonical-tuple pin.
        let samples: [Vec<Sexp>; 4] = [
            vec![],
            vec![Sexp::symbol("only")],
            vec![Sexp::int(1), Sexp::int(2), Sexp::int(3)],
            vec![
                Sexp::Nil,
                Sexp::Quote(Box::new(Sexp::symbol("x"))),
                Sexp::List(vec![Sexp::symbol("nested")]),
            ],
        ];
        for items in &samples {
            assert_eq!(
                Sexp::list(items.clone()).shape(),
                SexpShape::List,
                "Sexp::list→shape drifted from SexpShape::List for {items:?}",
            );
        }
    }

    #[test]
    fn sexp_list_constructor_composes_with_as_structural_kind() {
        // STRUCTURAL-KIND COMPOSITION LAW: every `Sexp::list(items)`
        // output projects through `Sexp::as_structural_kind` to
        // `Some(StructuralKind::List)` regardless of inner-item content
        // — the residual carving marker binds through the closed-set
        // `StructuralKind` algebra at ONE arm. Sibling-shape pin to the
        // atomic-axis's `Sexp::X_atom(payload).as_atom_kind() ==
        // Some(AtomKind::X)` marker composition. A regression that
        // reroutes `Sexp::list` through a non-residual arm (e.g. a copy-
        // edit that wraps items in `Sexp::Quote`) surfaces here through
        // the returned marker no longer being `StructuralKind::List`.
        let samples: [Vec<Sexp>; 4] = [
            vec![],
            vec![Sexp::symbol("only")],
            vec![Sexp::keyword("k"), Sexp::string("v")],
            vec![
                Sexp::Nil,
                Sexp::List(vec![Sexp::symbol("nested")]),
                Sexp::Unquote(Box::new(Sexp::symbol("var"))),
            ],
        ];
        for items in &samples {
            assert_eq!(
                Sexp::list(items.clone()).as_structural_kind(),
                Some(StructuralKind::List),
                "Sexp::list→as_structural_kind drifted from Some(StructuralKind::List) for {items:?}",
            );
        }
    }

    #[test]
    fn sexp_list_constructor_accepts_diverse_intoiterator_input_shapes() {
        // INPUT-SHAPE FLEXIBILITY: the `impl IntoIterator<Item = Sexp>`
        // bound accepts every reasonable owned-sequence shape without a
        // per-consumer `.collect::<Vec<Sexp>>()` coercion at the call
        // site — pin that `Vec<Sexp>`, `[Sexp; N]` array, `iter::empty
        // ::<Sexp>()`, and `.map(...)` iterator chains all reach the
        // same canonical tuple-variant output. A regression that
        // narrows the bound (e.g. taking `&[Sexp]` or `Vec<Sexp>` only)
        // fails this pin. The IntoIterator bound is load-bearing for the
        // ergonomy claim in the docstring — consumers threading a `.map`
        // chain through the outer algebra must not need an intermediate
        // `.collect()` before handing the result to `Sexp::list`.
        let expected = Sexp::List(vec![
            Sexp::symbol("a"),
            Sexp::symbol("b"),
            Sexp::symbol("c"),
        ]);
        // Vec<Sexp> — the canonical owned-sequence shape.
        assert_eq!(
            Sexp::list(vec![
                Sexp::symbol("a"),
                Sexp::symbol("b"),
                Sexp::symbol("c"),
            ]),
            expected,
            "Sexp::list drifted for Vec<Sexp> input",
        );
        // [Sexp; N] — array-literal shape (elements moved out of the
        // fixed-size array via the `IntoIterator` impl on `[T; N]`).
        assert_eq!(
            Sexp::list([Sexp::symbol("a"), Sexp::symbol("b"), Sexp::symbol("c"),]),
            expected,
            "Sexp::list drifted for [Sexp; N] input",
        );
        // `iter::empty::<Sexp>()` — the zero-item iterator shape.
        assert_eq!(
            Sexp::list(std::iter::empty::<Sexp>()),
            Sexp::List(vec![]),
            "Sexp::list drifted for iter::empty input",
        );
        // `.map(...)` iterator chain — the composition shape the
        // docstring's ergonomy claim rests on.
        assert_eq!(
            Sexp::list(["a", "b", "c"].iter().map(|s| Sexp::symbol(*s))),
            expected,
            "Sexp::list drifted for iterator-map chain input",
        );
        // `once(head).chain(tail)` — the head-then-rest shape a builder
        // consuming `head_symbol` + the tail slice threads through.
        assert_eq!(
            Sexp::list(
                std::iter::once(Sexp::symbol("a")).chain([Sexp::symbol("b"), Sexp::symbol("c")]),
            ),
            expected,
            "Sexp::list drifted for once+chain input",
        );
    }

    // ── Sexp::call — call-form (symbol-headed list) construct ──────────
    //
    // `Sexp::call(head, args)` is the section-for-retraction dual of the
    // soft-projection `Sexp::as_call() -> Option<(&str, &[Sexp])>` — it
    // embeds a fresh `(head string, item sequence)` pair into a symbol-
    // headed `Sexp::List` value at ONE site on the outer `Sexp` algebra,
    // composing the atomic-payload construct family's `Sexp::symbol` (for
    // the head position) with the residual-axis construct family's
    // `Sexp::list` (for the list wrapper) via `std::iter::once(head_sexp)
    // .chain(args)`. Pre-lift the composition lived inline at every
    // consumer that built a `(defX …)` typed-domain call form, a
    // macroexpander template head, or a synthetic dispatch form —
    // `Sexp::List(vec![Sexp::symbol(head), args...])` or `Sexp::List(
    // std::iter::once(Sexp::symbol(head)).chain(args).collect())` was the
    // welded three-method open coding. Post-lift the closure binds at
    // ONE typed-algebra method.
    //
    // These pins cover:
    //   (a) the composition law
    //       `Sexp::call(head, args) == Sexp::list(std::iter::once(
    //       Sexp::symbol(head)).chain(args))` — the constructor body is
    //       BY DEFINITION the two-method composition;
    //   (b) the round-trip law
    //       `Sexp::call(head, args.clone()).as_call() == Some((head,
    //       args.as_slice()))` — the (construct, project) call-form
    //       algebra dual closes at this pair, symmetric with the
    //       residual-axis's `Sexp::list(items.clone()).as_list() ==
    //       Some(items.as_slice())` round-trip;
    //   (c) the keyword-matched round-trip law
    //       `Sexp::call(head, args.clone()).as_call_to(head) == Some(
    //       args.as_slice())` — the keyword-typed projection recovers
    //       the args tail iff its argument matches the constructor's
    //       head;
    //   (d) the head-symbol composition law
    //       `Sexp::call(head, args).head_symbol() == Some(head)` — the
    //       head-position projection recovers the constructor's head
    //       byte-for-byte;
    //   (e) the outer-shape composition law
    //       `Sexp::call(head, args).shape() == SexpShape::List` — a
    //       call form is a list-shaped `Sexp`;
    //   (f) the structural-kind composition law
    //       `Sexp::call(head, args).as_structural_kind() == Some(
    //       StructuralKind::List)` — the residual carving marker binds
    //       through the closed-set `StructuralKind` algebra at ONE
    //       arm, symmetric with the residual-axis's `Sexp::list(items)
    //       .as_structural_kind() == Some(StructuralKind::List)` marker
    //       composition;
    //   (g) the input-shape flexibility
    //       `Sexp::call("h", Vec<Sexp>)` / `Sexp::call(String, [Sexp;
    //       N])` / `Sexp::call(&String, iter::map(...))` all agree with
    //       the canonical composition emission — the `impl Into<String>`
    //       head bound + `impl IntoIterator<Item = Sexp>` args bound
    //       accept every reasonable input shape without a per-consumer
    //       `.to_string()` / `.collect::<Vec<Sexp>>()` coercion.

    #[test]
    fn sexp_call_constructor_body_matches_canonical_two_method_composition_across_representative_inputs(
    ) {
        // COMPOSITION LAW: `Sexp::call(head, args) == Sexp::list(
        // std::iter::once(Sexp::symbol(head)).chain(args))` for every
        // representative (empty-args, single-arg, multi-arg,
        // heterogeneous-inner, quote-family-wrapping-inner) sample. A
        // regression that drifts the body (e.g. a copy-edit that
        // switches to `Sexp::keyword(head)` for the head position, or
        // that reorders `head` and `args` in the chain) surfaces here
        // BEFORE the projection pins fail. Sibling-shape pin to the
        // residual-axis's canonical-composition test posture
        // (`sexp_list_constructor_emits_canonical_tuple_variant_across_representative_inputs`).
        let samples: [(&str, Vec<Sexp>); 5] = [
            ("defcompiler", vec![]),
            ("defpoint", vec![Sexp::symbol("obs")]),
            (
                "defpoint",
                vec![
                    Sexp::symbol("obs"),
                    Sexp::keyword("class"),
                    Sexp::symbol("Gate"),
                ],
            ),
            (
                "defcheck",
                vec![
                    Sexp::List(vec![Sexp::symbol("crd-in-sync")]),
                    Sexp::keyword("params"),
                    Sexp::int(42),
                    Sexp::string("body"),
                    Sexp::boolean(true),
                ],
            ),
            (
                "defalert-policy",
                vec![
                    Sexp::Quote(Box::new(Sexp::symbol("x"))),
                    Sexp::Quasiquote(Box::new(Sexp::List(vec![
                        Sexp::symbol("template"),
                        Sexp::Unquote(Box::new(Sexp::symbol("var"))),
                    ]))),
                    Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))),
                ],
            ),
        ];
        for (head, args) in &samples {
            let expected =
                Sexp::list(std::iter::once(Sexp::symbol(*head)).chain(args.iter().cloned()));
            assert_eq!(
                Sexp::call(*head, args.clone()),
                expected,
                "Sexp::call drifted from Sexp::list(once(symbol(head)).chain(args)) for head={head:?} args={args:?}",
            );
        }
    }

    #[test]
    fn sexp_call_constructor_round_trips_through_as_call() {
        // ROUND-TRIP LAW (section-for-retraction with the call-form
        // soft-projection): `Sexp::call(head, args.clone()).as_call()
        // == Some((head, args.as_slice()))` sweeps the same
        // representative input matrix as the composition-law pin —
        // proves the (construct, soft-project) pair forms an
        // `Iso((&str, Vec<Sexp>), symbol-headed Sexp::List)` on the
        // call-form typed decomposition. Sibling-shape pin to the
        // residual-axis's `Sexp::list(items.clone()).as_list() ==
        // Some(items.as_slice())` round-trip
        // (`sexp_list_constructor_round_trips_through_as_list`).
        let samples: [(&str, Vec<Sexp>); 4] = [
            ("defcompiler", vec![]),
            ("defpoint", vec![Sexp::symbol("solo")]),
            (
                "defmonitor",
                vec![Sexp::symbol("m"), Sexp::int(1), Sexp::int(2)],
            ),
            (
                "defnotify",
                vec![
                    Sexp::Nil,
                    Sexp::List(vec![Sexp::symbol("nested"), Sexp::int(7)]),
                    Sexp::Quote(Box::new(Sexp::symbol("q"))),
                ],
            ),
        ];
        for (head, args) in &samples {
            let built = Sexp::call(*head, args.clone());
            assert_eq!(
                built.as_call(),
                Some((*head, args.as_slice())),
                "Sexp::call→as_call round-trip drifted for head={head:?} args={args:?}",
            );
        }
    }

    #[test]
    fn sexp_call_constructor_round_trips_through_as_call_to_matching_keyword() {
        // KEYWORD-MATCHED ROUND-TRIP LAW: `Sexp::call(head, args
        // .clone()).as_call_to(head) == Some(args.as_slice())` for the
        // head-matched keyword, and `.as_call_to(other)` returns `None`
        // for every other keyword. Pins the (construct, keyword-typed-
        // project) pair on the outer algebra — the same dispatch
        // shape `compile_typed` / `compile_named_from_forms` route
        // through post-macroexpansion.
        let samples: [(&str, Vec<Sexp>); 4] = [
            ("defcompiler", vec![]),
            ("defpoint", vec![Sexp::symbol("obs")]),
            ("defmonitor", vec![Sexp::keyword("k"), Sexp::string("v")]),
            (
                "defalert-policy",
                vec![Sexp::Nil, Sexp::List(vec![Sexp::symbol("body")])],
            ),
        ];
        for (head, args) in &samples {
            let built = Sexp::call(*head, args.clone());
            assert_eq!(
                built.as_call_to(head),
                Some(args.as_slice()),
                "Sexp::call→as_call_to(head) round-trip drifted for head={head:?} args={args:?}",
            );
            // Cross-keyword rejection: every DIFFERENT keyword misses.
            let mismatched = format!("{head}-mismatch");
            assert_eq!(
                built.as_call_to(&mismatched),
                None,
                "Sexp::call→as_call_to(mismatch) leaked args for head={head:?}",
            );
        }
    }

    #[test]
    fn sexp_call_constructor_composes_with_head_symbol_and_shape_and_structural_kind() {
        // OUTER-ALGEBRA PROJECTION COMPOSITIONS: every `Sexp::call(head,
        // args)` output projects through `head_symbol` /
        // `shape` / `as_structural_kind` to the shape-invariants that
        // pin the constructor's structural identity:
        //   * `head_symbol() == Some(head)` — the head-position
        //     projection recovers the constructor's head byte-for-byte;
        //   * `shape() == SexpShape::List` — a call form is a list-
        //     shaped `Sexp` on the residual carving;
        //   * `as_structural_kind() == Some(StructuralKind::List)` — the
        //     residual carving marker binds through the closed-set
        //     `StructuralKind` algebra at ONE arm.
        // A regression that reroutes `Sexp::call` through a non-list
        // arm (e.g. wrapping in `Sexp::Quote` after a copy-edit that
        // type-checks) fails ALL THREE pins simultaneously. Sibling to
        // the residual-axis's `Sexp::list` shape-composition pins
        // (`sexp_list_constructor_composes_with_shape_via_sexp_shape_list`
        // + `sexp_list_constructor_composes_with_as_structural_kind`).
        let samples: [(&str, Vec<Sexp>); 4] = [
            ("head", vec![]),
            ("head", vec![Sexp::symbol("only")]),
            (
                "head",
                vec![Sexp::keyword("k"), Sexp::string("v"), Sexp::boolean(false)],
            ),
            (
                "head",
                vec![
                    Sexp::Nil,
                    Sexp::Quote(Box::new(Sexp::symbol("x"))),
                    Sexp::List(vec![Sexp::symbol("nested")]),
                ],
            ),
        ];
        for (head, args) in &samples {
            let built = Sexp::call(*head, args.clone());
            assert_eq!(
                built.head_symbol(),
                Some(*head),
                "Sexp::call→head_symbol drifted from Some({head:?}) for args={args:?}",
            );
            assert_eq!(
                built.shape(),
                SexpShape::List,
                "Sexp::call→shape drifted from SexpShape::List for head={head:?} args={args:?}",
            );
            assert_eq!(
                built.as_structural_kind(),
                Some(StructuralKind::List),
                "Sexp::call→as_structural_kind drifted from Some(StructuralKind::List) for head={head:?} args={args:?}",
            );
        }
    }

    #[test]
    fn sexp_call_constructor_accepts_diverse_head_and_arg_input_shapes() {
        // INPUT-SHAPE FLEXIBILITY: the `impl Into<String>` head bound
        // absorbs `&str` / `String` / `&String`, and the `impl
        // IntoIterator<Item = Sexp>` args bound absorbs `Vec<Sexp>` /
        // `[Sexp; N]` / `iter::empty()` / `.map(...)` chains — pin that
        // all six representative input shapes reach the same canonical
        // composition output. A regression that narrows either bound
        // (e.g. requiring `String` on the head or `Vec<Sexp>` on the
        // args) fails this pin. The two bounds are load-bearing for the
        // ergonomy claim in the docstring — consumers threading a
        // borrowed head + a `.map` chain must not need `.to_string()` /
        // `.collect()` coercions before handing the pair to
        // `Sexp::call`. Sibling to `Sexp::list`'s input-shape pin
        // (`sexp_list_constructor_accepts_diverse_intoiterator_input_shapes`)
        // and `Sexp::symbol`'s head-string absorption posture.
        let expected = Sexp::List(vec![
            Sexp::symbol("head"),
            Sexp::symbol("a"),
            Sexp::symbol("b"),
        ]);
        // (&str, Vec<Sexp>) — the canonical borrowed-head + owned-args
        // shape.
        assert_eq!(
            Sexp::call("head", vec![Sexp::symbol("a"), Sexp::symbol("b")]),
            expected,
            "Sexp::call drifted for (&str, Vec<Sexp>) input",
        );
        // (String, [Sexp; N]) — the owned-head + array-literal shape.
        assert_eq!(
            Sexp::call(String::from("head"), [Sexp::symbol("a"), Sexp::symbol("b")],),
            expected,
            "Sexp::call drifted for (String, [Sexp; N]) input",
        );
        // (&String, .map(...)) — the borrowed-owned-head + iterator-map
        // chain shape.
        let owned_head = String::from("head");
        assert_eq!(
            Sexp::call(&owned_head, ["a", "b"].iter().map(|s| Sexp::symbol(*s))),
            expected,
            "Sexp::call drifted for (&String, iter::map) input",
        );
        // (&str, iter::empty::<Sexp>()) — the zero-arg iterator shape,
        // pinning the singleton-list emission (`(head)`) via the
        // composition path.
        assert_eq!(
            Sexp::call("head", std::iter::empty::<Sexp>()),
            Sexp::List(vec![Sexp::symbol("head")]),
            "Sexp::call drifted for zero-arg iter::empty input",
        );
        // (&str, once(head_of_args).chain(tail_of_args)) — the head-
        // then-rest args shape a builder decomposing an existing call
        // form via `as_call` and re-emitting through this constructor
        // threads through.
        assert_eq!(
            Sexp::call(
                "head",
                std::iter::once(Sexp::symbol("a")).chain([Sexp::symbol("b")]),
            ),
            expected,
            "Sexp::call drifted for (&str, once+chain) args input",
        );
    }

    #[test]
    fn sexp_call_constructor_body_matches_typed_composition_through_list_and_symbol() {
        // EXPLICIT COMPOSITION-LAW PIN: `Sexp::call(head, args) ==
        // Sexp::list(std::iter::once(Sexp::symbol(head)).chain(args))`
        // BY DEFINITION — the constructor body IS this composition, and
        // the pin exists so a regression that in-lines a hand-authored
        // `Sexp::List(vec![Sexp::symbol(head), args...])` body (which
        // would type-check and pass the projection round-trips) still
        // surfaces here through the composition-path drift. This closes
        // the "the constructor routes through the outer-algebra's
        // atomic + residual construct families" invariant as a typed
        // pin rather than a docstring claim.
        let head = "defpoint";
        let args = vec![
            Sexp::symbol("obs"),
            Sexp::keyword("class"),
            Sexp::List(vec![Sexp::symbol("Gate"), Sexp::symbol("Observability")]),
        ];
        assert_eq!(
            Sexp::call(head, args.clone()),
            Sexp::list(std::iter::once(Sexp::symbol(head)).chain(args.iter().cloned())),
            "Sexp::call body drifted from the Sexp::list ∘ once(Sexp::symbol) ∘ chain composition for head={head:?}",
        );
    }

    // ── Sexp::named_call — named-call-form (symbol-headed + NAME slot)
    //    construct ───────────────────────────────────────────────────────
    //
    // `Sexp::named_call(head, name, spec_args)` is the section-for-
    // retraction dual of the soft-projection `Sexp::as_named_call_to(
    // keyword) -> Option<Result<(&str, &[Sexp])>>` — it embeds a fresh
    // `(head string, NAME string, spec args sequence)` triple into a
    // symbol-headed `(head NAME spec_args…)` `Sexp::List` value at ONE
    // site on the outer `Sexp` algebra, composing the call-form
    // typed constructor `Sexp::call` (which itself composes the atomic
    // `Sexp::symbol` head with the residual `Sexp::list` wrapper) with
    // a NAME-slot `Sexp::symbol` embedding via `std::iter::once(
    // Sexp::symbol(name)).chain(spec_args)`. Pre-lift the composition
    // lived inline at every consumer that built a `(defX NAME …)`
    // typed-domain named authoring form or a synthetic named-dispatch
    // form — `Sexp::List(vec![Sexp::symbol(head), Sexp::symbol(name),
    // spec_args...])` or `Sexp::call(head, std::iter::once(
    // Sexp::symbol(name)).chain(spec_args))` was the welded quadruple
    // open coding. Post-lift the closure binds at ONE typed-algebra
    // method.
    //
    // These pins cover:
    //   (a) the composition law
    //       `Sexp::named_call(head, name, spec_args) == Sexp::call(
    //       head, std::iter::once(Sexp::symbol(name)).chain(spec_args))`
    //       — the constructor body is BY DEFINITION the two-method
    //       composition;
    //   (b) the round-trip law
    //       `Sexp::named_call(head, name, spec_args.clone())
    //       .as_named_call_to(head) == Some(Ok((name, spec_args
    //       .as_slice())))` — the (construct, named-project) named-
    //       call-form algebra dual closes at this pair, symmetric with
    //       the call-form's `Sexp::call(head, args.clone()).as_call()
    //       == Some((head, args.as_slice()))` round-trip;
    //   (c) the call-form projection composition
    //       `Sexp::named_call(head, name, spec_args)
    //       .as_call() == Some((head, [Sexp::symbol(name),
    //       spec_args…].as_slice()))` — the call-form soft-projection
    //       recovers `(head, [name, spec_args…])` with the NAME symbol
    //       as the first arg, threading the constructor's output
    //       through the encompassing call-form projection;
    //   (d) the keyword-matched round-trip law
    //       `Sexp::named_call(head, name, spec_args)
    //       .as_call_to(head) == Some([Sexp::symbol(name),
    //       spec_args…].as_slice())` — the keyword-typed projection
    //       recovers the NAME-headed args tail iff its argument
    //       matches the constructor's head;
    //   (e) the head-symbol composition law
    //       `Sexp::named_call(head, name, spec_args).head_symbol()
    //       == Some(head)` — the head-position projection recovers
    //       the constructor's head byte-for-byte;
    //   (f) the named-form gate composition law
    //       `crate::compile::split_name_slot(&Sexp::named_call(head,
    //       name, spec_args).as_call_to(head).unwrap(), head) == Ok((
    //       name, spec_args.as_slice()))` — the substrate's named-
    //       form arity + NAME-shape gate accepts every output of this
    //       constructor byte-for-byte, closing the section-for-
    //       retraction pair at the gate level as well as the
    //       projection level;
    //   (g) the outer-shape composition law
    //       `Sexp::named_call(head, name, spec_args).shape() ==
    //       SexpShape::List` and `.as_structural_kind() == Some(
    //       StructuralKind::List)` — the residual carving marker binds
    //       through the closed-set `StructuralKind` algebra at ONE
    //       arm, symmetric with `Sexp::call`'s residual-arm marker
    //       composition;
    //   (h) the input-shape flexibility
    //       `Sexp::named_call("h", "n", Vec<Sexp>)` / `Sexp::
    //       named_call(String, String, [Sexp; N])` / `Sexp::
    //       named_call(&str, &String, iter::map(...))` all agree with
    //       the canonical composition emission — the two `impl
    //       Into<String>` bounds + `impl IntoIterator<Item = Sexp>`
    //       args bound accept every reasonable input shape without a
    //       per-consumer `.to_string()` / `.collect::<Vec<Sexp>>()`
    //       coercion.

    #[test]
    fn sexp_named_call_constructor_body_matches_canonical_two_method_composition_across_representative_inputs(
    ) {
        // COMPOSITION LAW: `Sexp::named_call(head, name, spec_args) ==
        // Sexp::call(head, std::iter::once(Sexp::symbol(name)).chain(
        // spec_args))` for every representative (empty-spec-args,
        // single-spec-arg, multi-spec-arg, heterogeneous-inner,
        // quote-family-wrapping-inner) sample. A regression that
        // drifts the body (e.g. a copy-edit that switches to
        // `Sexp::keyword(name)` for the NAME position, or that
        // reorders `name` and `spec_args` in the chain) surfaces here
        // BEFORE the projection pins fail. Sibling-shape pin to
        // `Sexp::call`'s canonical-composition test posture.
        let samples: [(&'static str, &'static str, Vec<Sexp>); 5] = [
            ("defcompiler", "solo", vec![]),
            ("defpoint", "obs", vec![Sexp::keyword("class")]),
            (
                "defmonitor",
                "m",
                vec![
                    Sexp::keyword("severity"),
                    Sexp::symbol("Warning"),
                    Sexp::keyword("threshold"),
                    Sexp::int(42),
                ],
            ),
            (
                "defcheck",
                "coherent",
                vec![
                    Sexp::List(vec![Sexp::symbol("crd-in-sync")]),
                    Sexp::string("body"),
                    Sexp::boolean(true),
                ],
            ),
            (
                "defalert-policy",
                "outage",
                vec![
                    Sexp::Quote(Box::new(Sexp::symbol("x"))),
                    Sexp::Quasiquote(Box::new(Sexp::List(vec![
                        Sexp::symbol("template"),
                        Sexp::Unquote(Box::new(Sexp::symbol("var"))),
                    ]))),
                    Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))),
                ],
            ),
        ];
        for (head, name, spec_args) in &samples {
            let expected = Sexp::call(
                *head,
                std::iter::once(Sexp::symbol(*name)).chain(spec_args.iter().cloned()),
            );
            assert_eq!(
                Sexp::named_call(*head, *name, spec_args.clone()),
                expected,
                "Sexp::named_call drifted from Sexp::call(head, once(symbol(name)).chain(spec_args)) for head={head:?} name={name:?} spec_args={spec_args:?}",
            );
        }
    }

    #[test]
    fn sexp_named_call_constructor_round_trips_through_as_named_call_to() {
        // ROUND-TRIP LAW (section-for-retraction with the named-form
        // soft-projection): `Sexp::named_call(head, name, spec_args
        // .clone()).as_named_call_to(head) == Some(Ok((name,
        // spec_args.as_slice())))` sweeps the same representative
        // input matrix — proves the (construct, named-project) pair
        // forms an `Iso((&'static str, &str, Vec<Sexp>),
        // (head-symbol-headed + NAME-symbol-second Sexp::List))` on
        // the named-call-form typed decomposition. Sibling-shape pin
        // to `Sexp::call`'s round-trip through `as_call` posture.
        let samples: [(&'static str, &'static str, Vec<Sexp>); 4] = [
            ("defcompiler", "solo", vec![]),
            ("defpoint", "obs", vec![Sexp::keyword("class")]),
            (
                "defmonitor",
                "m",
                vec![Sexp::keyword("k"), Sexp::string("v")],
            ),
            (
                "defalert-policy",
                "outage",
                vec![Sexp::Nil, Sexp::List(vec![Sexp::symbol("body")])],
            ),
        ];
        for (head, name, spec_args) in &samples {
            let built = Sexp::named_call(*head, *name, spec_args.clone());
            assert_eq!(
                built.as_named_call_to(head).and_then(|res| res.ok()),
                Some((*name, spec_args.as_slice())),
                "Sexp::named_call→as_named_call_to round-trip drifted for head={head:?} name={name:?} spec_args={spec_args:?}",
            );
        }
    }

    #[test]
    fn sexp_named_call_constructor_projects_through_as_call_with_name_first_arg() {
        // CALL-FORM PROJECTION COMPOSITION: `Sexp::named_call(head,
        // name, spec_args).as_call() == Some((head, [Sexp::symbol(
        // name), spec_args…].as_slice()))` — the call-form soft-
        // projection recovers `(head, [name, spec_args…])` with the
        // NAME symbol as the first arg. Sibling-shape pin to the
        // call-form encompassing algebra: the named-call constructor
        // routes cleanly through the call-form projection AS A
        // COMPOSITION.
        let samples: [(&'static str, &'static str, Vec<Sexp>); 3] = [
            ("defcompiler", "solo", vec![]),
            ("defpoint", "obs", vec![Sexp::keyword("class")]),
            (
                "defmonitor",
                "m",
                vec![Sexp::keyword("threshold"), Sexp::int(42)],
            ),
        ];
        for (head, name, spec_args) in &samples {
            let built = Sexp::named_call(*head, *name, spec_args.clone());
            let expected_args: Vec<Sexp> = std::iter::once(Sexp::symbol(*name))
                .chain(spec_args.iter().cloned())
                .collect();
            assert_eq!(
                built.as_call(),
                Some((*head, expected_args.as_slice())),
                "Sexp::named_call→as_call drifted for head={head:?} name={name:?} spec_args={spec_args:?}",
            );
        }
    }

    #[test]
    fn sexp_named_call_constructor_round_trips_through_as_call_to_matching_keyword() {
        // KEYWORD-MATCHED ROUND-TRIP LAW: `Sexp::named_call(head,
        // name, spec_args.clone()).as_call_to(head) == Some([
        // Sexp::symbol(name), spec_args…].as_slice())` for the head-
        // matched keyword, and `.as_call_to(other) == None` for every
        // other keyword. Pins the (construct, keyword-typed-project)
        // pair on the outer algebra threading through the NAMED axis
        // — the same dispatch shape `compile_named_from_forms` routes
        // through post-macroexpansion.
        let samples: [(&'static str, &'static str, Vec<Sexp>); 3] = [
            ("defcompiler", "solo", vec![]),
            ("defpoint", "obs", vec![Sexp::keyword("class")]),
            (
                "defmonitor",
                "m",
                vec![Sexp::keyword("k"), Sexp::string("v")],
            ),
        ];
        for (head, name, spec_args) in &samples {
            let built = Sexp::named_call(*head, *name, spec_args.clone());
            let expected_args: Vec<Sexp> = std::iter::once(Sexp::symbol(*name))
                .chain(spec_args.iter().cloned())
                .collect();
            assert_eq!(
                built.as_call_to(head),
                Some(expected_args.as_slice()),
                "Sexp::named_call→as_call_to(head) round-trip drifted for head={head:?} name={name:?} spec_args={spec_args:?}",
            );
            // Cross-keyword rejection: every DIFFERENT keyword misses.
            let mismatched = format!("{head}-mismatch");
            assert_eq!(
                built.as_call_to(&mismatched),
                None,
                "Sexp::named_call→as_call_to(mismatch) leaked args for head={head:?} name={name:?}",
            );
        }
    }

    #[test]
    fn sexp_named_call_constructor_composes_with_head_symbol_and_shape_and_structural_kind() {
        // OUTER-ALGEBRA PROJECTION COMPOSITIONS: every `Sexp::
        // named_call(head, name, spec_args)` output projects through
        // `head_symbol` / `shape` / `as_structural_kind` to the shape-
        // invariants that pin the constructor's structural identity:
        //   * `head_symbol() == Some(head)` — the head-position
        //     projection recovers the constructor's head byte-for-byte;
        //   * `shape() == SexpShape::List` — a named call form is a
        //     list-shaped `Sexp` on the residual carving;
        //   * `as_structural_kind() == Some(StructuralKind::List)` —
        //     the residual carving marker binds through the closed-
        //     set `StructuralKind` algebra at ONE arm.
        let samples: [(&'static str, &'static str, Vec<Sexp>); 3] = [
            ("head", "n", vec![]),
            ("head", "n", vec![Sexp::keyword("k"), Sexp::string("v")]),
            (
                "head",
                "n",
                vec![
                    Sexp::Nil,
                    Sexp::Quote(Box::new(Sexp::symbol("x"))),
                    Sexp::List(vec![Sexp::symbol("nested")]),
                ],
            ),
        ];
        for (head, name, spec_args) in &samples {
            let built = Sexp::named_call(*head, *name, spec_args.clone());
            assert_eq!(
                built.head_symbol(),
                Some(*head),
                "Sexp::named_call→head_symbol drifted from Some({head:?}) for name={name:?} spec_args={spec_args:?}",
            );
            assert_eq!(
                built.shape(),
                SexpShape::List,
                "Sexp::named_call→shape drifted from SexpShape::List for head={head:?} name={name:?} spec_args={spec_args:?}",
            );
            assert_eq!(
                built.as_structural_kind(),
                Some(StructuralKind::List),
                "Sexp::named_call→as_structural_kind drifted from Some(StructuralKind::List) for head={head:?} name={name:?} spec_args={spec_args:?}",
            );
        }
    }

    #[test]
    fn sexp_named_call_constructor_output_passes_the_split_name_slot_gate() {
        // NAMED-FORM GATE COMPOSITION LAW: `crate::compile::
        // split_name_slot(&Sexp::named_call(head, name, spec_args)
        // .as_call_to(head).unwrap(), head) == Ok((name, spec_args
        // .as_slice()))` — the substrate's named-form arity + NAME-
        // shape gate accepts every output of this constructor byte-
        // for-byte, closing the section-for-retraction pair at the
        // GATE level as well as the projection level. A regression
        // that emits a value the gate rejects (e.g. a
        // `NamedFormNonSymbolName` from a `Sexp::keyword(name)` NAME
        // slot copy-edit) surfaces here even when the projection
        // pins pass.
        let samples: [(&'static str, &'static str, Vec<Sexp>); 4] = [
            ("defcompiler", "solo", vec![]),
            ("defpoint", "obs", vec![Sexp::keyword("class")]),
            (
                "defmonitor",
                "m",
                vec![Sexp::keyword("severity"), Sexp::symbol("Warning")],
            ),
            (
                "defalert-policy",
                "outage",
                vec![
                    Sexp::List(vec![Sexp::symbol("body")]),
                    Sexp::Quote(Box::new(Sexp::symbol("x"))),
                ],
            ),
        ];
        for (head, name, spec_args) in &samples {
            let built = Sexp::named_call(*head, *name, spec_args.clone());
            let args_tail = built
                .as_call_to(head)
                .expect("Sexp::named_call output must pass Sexp::as_call_to(head)");
            let gated = crate::compile::split_name_slot(args_tail, head)
                .expect("Sexp::named_call output must pass split_name_slot");
            assert_eq!(
                gated,
                (*name, spec_args.as_slice()),
                "Sexp::named_call→split_name_slot round-trip drifted for head={head:?} name={name:?} spec_args={spec_args:?}",
            );
        }
    }

    #[test]
    fn sexp_named_call_constructor_accepts_diverse_head_name_and_arg_input_shapes() {
        // INPUT-SHAPE FLEXIBILITY: the two `impl Into<String>` bounds
        // absorb `&str` / `String` / `&String` on both head + NAME
        // positions, and the `impl IntoIterator<Item = Sexp>` spec-
        // args bound absorbs `Vec<Sexp>` / `[Sexp; N]` / `iter::
        // empty()` / `.map(...)` chains — pin that all five
        // representative input shapes reach the same canonical
        // composition output. A regression that narrows any bound
        // fails this pin. Sibling to `Sexp::call`'s input-shape pin.
        let expected = Sexp::List(vec![
            Sexp::symbol("head"),
            Sexp::symbol("name"),
            Sexp::symbol("a"),
            Sexp::symbol("b"),
        ]);
        // (&str, &str, Vec<Sexp>) — the canonical borrowed shape.
        assert_eq!(
            Sexp::named_call("head", "name", vec![Sexp::symbol("a"), Sexp::symbol("b")]),
            expected,
            "Sexp::named_call drifted for (&str, &str, Vec<Sexp>) input",
        );
        // (String, String, [Sexp; N]) — the owned + array-literal
        // shape.
        assert_eq!(
            Sexp::named_call(
                String::from("head"),
                String::from("name"),
                [Sexp::symbol("a"), Sexp::symbol("b")],
            ),
            expected,
            "Sexp::named_call drifted for (String, String, [Sexp; N]) input",
        );
        // (&str, &String, .map(...)) — the borrowed-owned-name +
        // iterator-map chain shape.
        let owned_name = String::from("name");
        assert_eq!(
            Sexp::named_call(
                "head",
                &owned_name,
                ["a", "b"].iter().map(|s| Sexp::symbol(*s))
            ),
            expected,
            "Sexp::named_call drifted for (&str, &String, iter::map) input",
        );
        // (&str, &str, iter::empty::<Sexp>()) — the zero-spec-args
        // iterator shape, pinning the two-element list emission
        // (`(head name)`) via the composition path.
        assert_eq!(
            Sexp::named_call("head", "name", std::iter::empty::<Sexp>()),
            Sexp::List(vec![Sexp::symbol("head"), Sexp::symbol("name")]),
            "Sexp::named_call drifted for zero-spec-args iter::empty input",
        );
        // (&str, &str, once+chain) — the head-then-rest spec-args
        // shape a builder decomposing an existing named call form
        // via `as_named_call_to` and re-emitting through this
        // constructor threads through.
        assert_eq!(
            Sexp::named_call(
                "head",
                "name",
                std::iter::once(Sexp::symbol("a")).chain([Sexp::symbol("b")]),
            ),
            expected,
            "Sexp::named_call drifted for (&str, &str, once+chain) spec-args input",
        );
    }

    #[test]
    fn sexp_named_call_constructor_body_matches_typed_composition_through_call_and_symbol() {
        // EXPLICIT COMPOSITION-LAW PIN: `Sexp::named_call(head, name,
        // spec_args) == Sexp::call(head, std::iter::once(Sexp::symbol(
        // name)).chain(spec_args))` BY DEFINITION — the constructor
        // body IS this composition, and the pin exists so a
        // regression that in-lines a hand-authored `Sexp::List(vec![
        // Sexp::symbol(head), Sexp::symbol(name), spec_args...])`
        // body (which would type-check and pass the projection round-
        // trips) still surfaces here through the composition-path
        // drift. Closes the "the constructor routes through
        // `Sexp::call` + `Sexp::symbol`" invariant as a typed pin
        // rather than a docstring claim.
        let head = "defpoint";
        let name = "observability-stack";
        let spec_args = vec![
            Sexp::keyword("class"),
            Sexp::List(vec![Sexp::symbol("Gate"), Sexp::symbol("Observability")]),
        ];
        assert_eq!(
            Sexp::named_call(head, name, spec_args.clone()),
            Sexp::call(
                head,
                std::iter::once(Sexp::symbol(name)).chain(spec_args.iter().cloned()),
            ),
            "Sexp::named_call body drifted from the Sexp::call ∘ once(Sexp::symbol) ∘ chain composition for head={head:?} name={name:?}",
        );
    }
}
