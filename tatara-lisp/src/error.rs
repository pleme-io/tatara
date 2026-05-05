use thiserror::Error;

pub type Result<T> = std::result::Result<T, LispError>;

#[derive(Debug, Error)]
pub enum LispError {
    #[error("unexpected character {0:?} at position {1}")]
    UnexpectedChar(char, usize),
    #[error("unterminated string literal at position {0}")]
    UnterminatedString(usize),
    #[error("unmatched closing paren at position {pos}")]
    UnmatchedParen { pos: usize },
    #[error("unmatched opening paren at position {pos}")]
    UnmatchedOpenParen { pos: usize },
    #[error("unexpected end of input at position {pos}")]
    Eof { pos: usize },
    #[error("invalid number literal {0:?}")]
    InvalidNumber(String),
    #[error("unknown symbol: {0}")]
    UnknownSymbol(String),
    #[error("type error: expected {expected}, got {got}")]
    Type { expected: &'static str, got: String },
    #[error("compile error in {form}: {message}")]
    Compile { form: String, message: String },
    /// Structured type mismatch — both sides are first-class fields, not
    /// embedded substrings of `message`. Rendered as `"compile error in
    /// {form}: expected {expected}, got {got}"` so the user-facing string
    /// matches the legacy `Compile`-shaped diagnostic byte-for-byte; the
    /// gain is structural — authoring tools (REPL, LSP, `tatara-check`)
    /// pattern-match on the variant and bind directly to `expected` /
    /// `got` instead of substring-parsing the rendered message.
    ///
    /// `expected` is `&'static str` so a typo can never drift into the
    /// diagnostic at runtime; `got` is `&'static str` because it is
    /// always the output of `crate::domain::sexp_type_name`, whose match
    /// is exhaustive over `Sexp` at compile time. When a future run gives
    /// `Sexp` source spans, `pos: Option<usize>` lands here in ONE place
    /// and every type-mismatch site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in {form}: expected {expected}, got {got}")]
    TypeMismatch {
        form: String,
        expected: &'static str,
        got: &'static str,
    },
    /// Structural head-mismatch — the `(head ...)` of a top-level form
    /// didn't match `T::KEYWORD`. Both sides are first-class fields, not
    /// embedded substrings of `message`. Rendered as `"compile error in
    /// {keyword}: expected ({keyword} ...), got ({got} ...)"` so the
    /// user-facing string matches the legacy `Compile`-shaped diagnostic
    /// byte-for-byte; the gain is structural — authoring tools (REPL,
    /// LSP, `tatara-check`) pattern-match on the variant and bind
    /// directly to `keyword` / `got` instead of substring-parsing the
    /// rendered message.
    ///
    /// `keyword` is `&'static str` because it always comes from
    /// `T::KEYWORD`, a compile-time literal; `got` is `String` because
    /// it is an arbitrary symbol from the source. When a future run
    /// gives `Sexp` source spans, `pos: Option<usize>` lands here in
    /// ONE place and every head-mismatch site picks up positional
    /// rendering via `crate::diagnostic::format_diagnostic`
    /// mechanically.
    #[error("compile error in {keyword}: expected ({keyword} ...), got ({got} ...)")]
    HeadMismatch { keyword: &'static str, got: String },
    #[error("unknown {category}: {value}")]
    Unknown {
        category: &'static str,
        value: String,
    },
    #[error("missing required field: {0}")]
    Missing(&'static str),
    /// A kwargs list of odd length: the last element has no partner. The
    /// `dangling` field holds the offending element's `Sexp::Display`
    /// projection — `:query` for a keyword whose value got lost, or the
    /// literal form of a stray non-keyword. Naming both halves of the
    /// failure (the failure mode AND the offending element) is the
    /// typed-entry gate's structural-completeness floor (THEORY.md §V.1):
    /// without it the operator must re-read the source to find what
    /// actually misfired.
    #[error("odd keyword arguments: dangling element `{dangling}`")]
    OddKwargs { dangling: String },
    /// An unquote (`,name`) or unquote-splice (`,@name`) in a macro template
    /// body referenced a name that wasn't bound by the macro's params (during
    /// `compile_template`) or wasn't available in the substitution scope
    /// (during `substitute`). `prefix` is the syntactic marker — `","` for
    /// unquote, `",@"` for splice — so the rendered diagnostic preserves the
    /// form exactly as the author wrote it. `hint` is `Some(name)` when the
    /// substrate found a near-miss against the available bound names within
    /// the bounded edit distance (see `crate::domain::suggest`); `None`
    /// otherwise — a wrong hint is worse than no hint, so the slot stays
    /// empty unless the substrate is confident.
    ///
    /// `prefix` is `&'static str` because every call site passes a literal
    /// (`","` or `",@"`); a typo in the marker can never drift into the
    /// diagnostic at runtime — the type system is the floor. `name` and
    /// `hint` are `String` because they come from arbitrary source / the
    /// live bindings set. When a future run gives `Sexp` source spans, `pos:
    /// Option<usize>` lands here in ONE place and every unbound-template-var
    /// site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    ///
    /// Display matches the legacy `Compile`-shaped diagnostic byte-for-byte
    /// when `hint` is `None` — `"compile error in {prefix}{name}: unbound"`
    /// — so existing consumer assertions that pattern-match on the message
    /// substring keep passing. With a hint, the suffix `"; did you mean
    /// {prefix}{hint}?"` is appended; the prefix is preserved in the hint so
    /// the operator can copy-paste the suggestion verbatim.
    #[error(
        "compile error in {prefix}{name}: unbound{}",
        unbound_hint_suffix(prefix, hint.as_deref())
    )]
    UnboundTemplateVar {
        prefix: &'static str,
        name: String,
        hint: Option<String>,
    },
    /// A kwargs slice contained the same `:key` twice. The offending key is
    /// carried as a structural field — not embedded in a free-form message —
    /// so authoring surfaces (REPL, LSP, `tatara-check`) pattern-match on
    /// the variant and bind to `key` directly instead of substring-parsing
    /// the rendered diagnostic. Same posture as the `OddKwargs { dangling }`
    /// sibling: every distinct `parse_kwargs` failure mode (odd length,
    /// not-a-keyword-at-position, duplicate key) is now a structural variant
    /// of `LispError`, not a `Compile`-shaped substring.
    ///
    /// `key` is `String` because it comes from arbitrary source. Display
    /// renders `"compile error in :{key}: duplicate keyword"` byte-for-byte
    /// equivalent to the legacy `Compile { form: kwarg_form(key), message:
    /// "duplicate keyword" }` shape, so existing consumer assertions
    /// (`msg.contains(":name")`, `msg.contains("duplicate keyword")`) pass
    /// unchanged. When a future run gives `Sexp` source spans, `pos:
    /// Option<usize>` lands here in ONE place and every duplicate-kwarg
    /// site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in :{key}: duplicate keyword")]
    DuplicateKwarg { key: String },
    /// A required kwarg was absent from the kwargs slice. The offending key
    /// is carried as a structural field — not embedded in a free-form
    /// message — so authoring surfaces (REPL, LSP, `tatara-check`)
    /// pattern-match on the variant and bind to `key` directly instead of
    /// substring-parsing the rendered diagnostic. Same posture as
    /// `DuplicateKwarg { key }` and `OddKwargs { dangling }`: every
    /// distinct typed-entry kwarg failure mode is now a structural variant
    /// of `LispError`, not a `Compile`-shaped substring. Sibling of the
    /// pre-existing `Missing(&'static str)` variant — `MissingKwarg`
    /// covers the runtime-key path the kwargs extractors share, while
    /// `Missing` stays for compile-time-known names.
    ///
    /// `key` is `String` because it comes from the runtime kwargs lookup
    /// (each derive-generated extractor and every hand-written
    /// `TataraDomain` impl can pass an arbitrary key). Display renders
    /// `"compile error in :{key}: required but not provided"`
    /// byte-for-byte equivalent to the legacy `Compile { form:
    /// kwarg_form(key), message: "required but not provided" }` shape, so
    /// existing consumer assertions (`msg.contains(":threshold")`,
    /// `msg.contains("required")`) pass unchanged. When a future run gives
    /// `Sexp` source spans, `pos: Option<usize>` lands here in ONE place
    /// and every missing-kwarg site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in :{key}: required but not provided")]
    MissingKwarg { key: String },
    /// A kwargs slice contained a `:key` that isn't in the allowed-kwarg
    /// set for the surrounding `TataraDomain`. The offending key, the
    /// near-miss hint (if any), and the full allowed set are all carried
    /// as first-class fields — not embedded in a free-form message — so
    /// authoring surfaces (REPL, LSP, `tatara-check`) pattern-match on
    /// the variant and bind to `key` / `hint` / `allowed` directly
    /// instead of substring-parsing the rendered message. Same posture
    /// as the `OddKwargs { dangling }`, `DuplicateKwarg { key }`, and
    /// `MissingKwarg { key }` siblings: every distinct typed-entry
    /// kwarg-gate failure mode is now a structural variant of
    /// `LispError`, not a `Compile`-shaped substring.
    ///
    /// `key` is `String` because it comes from arbitrary source. `hint`
    /// is `Some(allowed_keyword)` when `crate::domain::suggest` ranks an
    /// allowed kwarg within the bounded edit distance; `None`
    /// otherwise — a wrong hint is worse than no hint, so the slot
    /// stays empty unless the substrate is confident. `allowed` is
    /// `Vec<String>` (sorted lexicographically by `unknown_kwarg`)
    /// because the variant owns its data — the derive-emitted
    /// `&'static [&'static str]` allowed-set crosses the structural
    /// boundary as owned `String`s so the diagnostic crosses thread
    /// boundaries cleanly and lives independent of the call frame.
    /// Display matches the legacy `Compile { form: kwarg_form(key),
    /// message: "unknown keyword (...)" }` rendering byte-for-byte
    /// (`"compile error in :{key}: unknown keyword (did you mean
    /// :{hint}?; allowed: :a, :b, :c)"` with a hint, `"compile error
    /// in :{key}: unknown keyword (allowed: :a, :b, :c)"` without),
    /// so existing consumer assertions
    /// (`msg.contains("unknown keyword")`,
    /// `msg.contains("did you mean :threshold?")`,
    /// `msg.contains("allowed: ")`) pass unchanged. When a future
    /// run gives `Sexp` source spans, `pos: Option<usize>` lands
    /// here in ONE place and every unknown-kwarg site picks up
    /// positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error(
        "compile error in :{key}: unknown keyword{}",
        unknown_kwarg_suffix(hint.as_deref(), allowed)
    )]
    UnknownKwarg {
        key: String,
        hint: Option<String>,
        allowed: Vec<String>,
    },
    /// A registry-dispatched form `(<keyword> ...)` whose head symbol isn't in
    /// the global `TataraDomain` registry. The offending keyword, the
    /// near-miss hint (if any), and the full registered keyword set are all
    /// carried as first-class fields — not embedded in a free-form message —
    /// so authoring surfaces (REPL, LSP, `tatara-check`) pattern-match on the
    /// variant and bind to `keyword` / `hint` / `registered` directly instead
    /// of substring-parsing the rendered message. Same posture as the
    /// `UnknownKwarg { key, hint, allowed }` sibling: the kwarg-gate's
    /// unknown-allowed-set rejection and the registry-gate's
    /// unknown-registered-set rejection share ONE structural shape.
    ///
    /// `keyword` is `String` because it comes from arbitrary source (a
    /// top-level form's head symbol). `hint` is
    /// `Some(registered_keyword)` when `crate::domain::suggest_keyword`
    /// ranks a registered keyword within the bounded edit distance;
    /// `None` otherwise — a wrong hint is worse than no hint, so the slot
    /// stays empty unless the substrate is confident. `registered` is
    /// `Vec<String>` (sorted lexicographically by
    /// `unknown_domain_keyword`) because the variant owns its data — the
    /// registry's `&'static [&'static str]` keyword-set crosses the
    /// structural boundary as owned `String`s so the diagnostic crosses
    /// thread boundaries cleanly and lives independent of the call frame.
    /// Empty `registered` (no domains seeded) renders `(no domains
    /// registered)` so the operator sees the structural reason — the
    /// registry has no candidates at all — instead of a misleading empty
    /// "registered: " suffix. When a future run gives `Sexp` source
    /// spans, `pos: Option<usize>` lands here in ONE place and every
    /// unknown-domain-keyword site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error(
        "unknown domain keyword: ({keyword} ...){}",
        unknown_domain_keyword_suffix(hint.as_deref(), registered)
    )]
    UnknownDomainKeyword {
        keyword: String,
        hint: Option<String>,
        registered: Vec<String>,
    },
    /// The slot inside a `Sexp::Unquote(_)` (`,X`) or
    /// `Sexp::UnquoteSplice(_)` (`,@X`) was not a symbol. The `prefix`
    /// field is the syntactic marker — `","` for unquote, `",@"` for
    /// splice — so the rendered diagnostic preserves the form exactly
    /// as the author wrote it. The `got` field is the offending inner's
    /// `Sexp::Display` projection so the operator sees both what was
    /// expected (a symbol — the only form a no-evaluator template can
    /// substitute) and what was actually written (the literal value —
    /// `(list 1 2)`, `5`, `:foo`, etc.). Naming both halves of the
    /// failure is the typed-entry gate's structural-completeness floor
    /// (THEORY.md §V.1).
    ///
    /// Sibling of `UnboundTemplateVar { prefix, name, hint }` for the
    /// same template-side typed-entry surface — that variant fires when
    /// the slot IS a symbol but the symbol isn't bound; this variant
    /// fires when the slot isn't a symbol at all. After this lift every
    /// distinct typed-entry template-gate failure mode binds to ONE
    /// structural variant of `LispError`, not a `Compile`-shaped
    /// substring.
    ///
    /// `prefix` is `&'static str` because every call site passes a
    /// literal (`","` / `",@"`); a typo in the marker can never drift
    /// into the diagnostic at runtime — the type system is the floor.
    /// `got` is `String` because it comes from arbitrary source via
    /// `Sexp::Display`. When a future run gives `Sexp` source spans,
    /// `pos: Option<usize>` lands here in ONE place and every
    /// non-symbol-unquote-target site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in {prefix}: expected symbol, got {got}")]
    NonSymbolUnquoteTarget { prefix: &'static str, got: String },
    /// A `,@X` (unquote-splice) appeared at a syntactic position where there
    /// is no containing list to splice into — i.e. the splice is the entire
    /// macro-template body, not nested inside a `(... ,@xs ...)` list. Splice
    /// is always list-flattening: `,@(a b c)` inside `(outer ,@xs)` becomes
    /// `(outer a b c)`. At a non-list position there is no list to flatten
    /// into; the form is ill-positioned regardless of whether the inner slot
    /// is a symbol, a literal, or a bound list.
    ///
    /// Sibling of `NonSymbolUnquoteTarget { prefix, got }` and
    /// `UnboundTemplateVar { prefix, name, hint }` for the template-gate's
    /// other distinct failure modes — together the three close every
    /// distinct typed-entry template-gate failure mode for the no-evaluator
    /// template language: each is a structural variant of `LispError`, not
    /// a `Compile`-shaped substring. `prefix` is implicit (always `,@`) and
    /// elided from the variant: this failure mode names ONE syntactic
    /// marker, parallel to how `OddKwargs` names ONE failure mode (odd-length
    /// kwargs slice) without a syntactic-marker slot.
    ///
    /// `got` is `String` because it comes from arbitrary source via
    /// `Sexp::Display` (the offending inner — `xs`, `(list 1 2)`, `5`,
    /// `:foo`, etc.). Naming both the failure mode AND the offending element
    /// is the typed-entry gate's structural-completeness floor (THEORY.md
    /// §V.1) — without it the operator must re-read the source to find what
    /// actually misfired. When a future run gives `Sexp` source spans, `pos:
    /// Option<usize>` lands here in ONE place and every splice-outside-list
    /// site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    ///
    /// Display renders `"compile error in ,@: \`,@\` may only appear inside
    /// a list (got ,@{got})"` — the legacy substring `"\`,@\` may only
    /// appear inside a list"` is preserved verbatim so authoring tools that
    /// substring-match on the rendered diagnostic see no drift; the
    /// parenthetical `(got ,@{got})` names the offending form so an LSP
    /// quick-fix that surfaces "the splice has no containing list; you
    /// wrote `,@xs`" gains the literal value as data, no message re-parsing
    /// required.
    #[error("compile error in ,@: `,@` may only appear inside a list (got ,@{got})")]
    SpliceOutsideList { got: String },
}

fn unbound_hint_suffix(prefix: &str, hint: Option<&str>) -> String {
    match hint {
        Some(h) => format!("; did you mean {prefix}{h}?"),
        None => String::new(),
    }
}

fn unknown_kwarg_suffix(hint: Option<&str>, allowed: &[String]) -> String {
    let allowed_list = allowed
        .iter()
        .map(|s| format!(":{s}"))
        .collect::<Vec<_>>()
        .join(", ");
    match hint {
        Some(h) => format!(" (did you mean :{h}?; allowed: {allowed_list})"),
        None => format!(" (allowed: {allowed_list})"),
    }
}

fn unknown_domain_keyword_suffix(hint: Option<&str>, registered: &[String]) -> String {
    if registered.is_empty() {
        return match hint {
            Some(h) => format!(" (did you mean ({h} ...)?; no domains registered)"),
            None => " (no domains registered)".into(),
        };
    }
    let registered_list = registered.join(", ");
    match hint {
        Some(h) => format!(" (did you mean ({h} ...)?; registered: {registered_list})"),
        None => format!(" (registered: {registered_list})"),
    }
}

impl LispError {
    /// Byte offset of the failure into the source, when locatable.
    ///
    /// Variants without a position (`Type`, `Compile`, etc.) return `None`,
    /// so callers can render a snippet only when the substrate has the
    /// information to do so. New positional variants gain editor-ready
    /// rendering (via `crate::diagnostic::format_diagnostic`) by adding a
    /// branch here — no consumer changes required.
    #[must_use]
    pub fn position(&self) -> Option<usize> {
        match self {
            Self::UnexpectedChar(_, pos) | Self::UnterminatedString(pos) => Some(*pos),
            Self::UnmatchedParen { pos } | Self::UnmatchedOpenParen { pos } | Self::Eof { pos } => {
                Some(*pos)
            }
            Self::InvalidNumber(_)
            | Self::UnknownSymbol(_)
            | Self::Type { .. }
            | Self::Compile { .. }
            | Self::TypeMismatch { .. }
            | Self::HeadMismatch { .. }
            | Self::Unknown { .. }
            | Self::Missing(_)
            | Self::OddKwargs { .. }
            | Self::UnboundTemplateVar { .. }
            | Self::DuplicateKwarg { .. }
            | Self::MissingKwarg { .. }
            | Self::UnknownKwarg { .. }
            | Self::UnknownDomainKeyword { .. }
            | Self::NonSymbolUnquoteTarget { .. }
            | Self::SpliceOutsideList { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LispError;

    #[test]
    fn position_extracts_offset_from_positional_variants() {
        assert_eq!(LispError::UnexpectedChar('?', 7).position(), Some(7));
        assert_eq!(LispError::UnterminatedString(11).position(), Some(11));
        assert_eq!(LispError::UnmatchedParen { pos: 3 }.position(), Some(3));
        assert_eq!(LispError::UnmatchedOpenParen { pos: 0 }.position(), Some(0));
        assert_eq!(LispError::Eof { pos: 42 }.position(), Some(42));
    }

    #[test]
    fn position_is_none_for_non_positional_variants() {
        assert_eq!(
            LispError::OddKwargs {
                dangling: ":query".into()
            }
            .position(),
            None
        );
        assert_eq!(LispError::Missing("name").position(), None);
        assert_eq!(LispError::InvalidNumber("nan".into()).position(), None);
        assert_eq!(LispError::UnknownSymbol("foo".into()).position(), None);
        assert_eq!(
            LispError::Type {
                expected: "int",
                got: "string".into()
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::Compile {
                form: ":x".into(),
                message: "bad".into()
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::TypeMismatch {
                form: ":x".into(),
                expected: "string",
                got: "int",
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::HeadMismatch {
                keyword: "defmonitor",
                got: "not-a-monitor".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::Unknown {
                category: "domain",
                value: "defx".into()
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::UnboundTemplateVar {
                prefix: ",",
                name: "xx".into(),
                hint: Some("x".into()),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::UnboundTemplateVar {
                prefix: ",@",
                name: "ys".into(),
                hint: None,
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::DuplicateKwarg { key: "name".into() }.position(),
            None
        );
        assert_eq!(
            LispError::MissingKwarg { key: "name".into() }.position(),
            None
        );
        assert_eq!(
            LispError::UnknownKwarg {
                key: "tthreshold".into(),
                hint: Some("threshold".into()),
                allowed: vec!["name".into(), "threshold".into()],
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::UnknownDomainKeyword {
                keyword: "defmoniter".into(),
                hint: Some("defmonitor".into()),
                registered: vec!["defalertpolicy".into(), "defmonitor".into()],
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::NonSymbolUnquoteTarget {
                prefix: ",",
                got: "(list 1 2)".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::NonSymbolUnquoteTarget {
                prefix: ",@",
                got: "5".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::SpliceOutsideList { got: "xs".into() }.position(),
            None
        );
        assert_eq!(
            LispError::SpliceOutsideList {
                got: "(list 1 2)".into(),
            }
            .position(),
            None
        );
    }

    #[test]
    fn unknown_kwarg_display_with_hint_renders_did_you_mean_then_allowed_list() {
        // The variant renders byte-for-byte the same string the legacy
        // `Compile { form: ":tthreshold", message: "unknown keyword (did
        // you mean :threshold?; allowed: :a, :b, :c)" }` shape produced,
        // so authoring tools (REPL, LSP, `tatara-check`) that
        // substring-match on the rendered diagnostic see no drift; tools
        // that pattern-match on the variant gain structural binding.
        let err = LispError::UnknownKwarg {
            key: "tthreshold".into(),
            hint: Some("threshold".into()),
            allowed: vec!["name".into(), "query".into(), "threshold".into()],
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :tthreshold: unknown keyword \
             (did you mean :threshold?; allowed: :name, :query, :threshold)"
        );
    }

    #[test]
    fn unknown_kwarg_display_without_hint_renders_allowed_list_only() {
        // No hint: the rendered message has the allowed list but no `did
        // you mean` clause. A wrong hint is worse than no hint — the
        // slot stays empty unless `suggest` ranks a candidate within
        // the bounded edit distance.
        let err = LispError::UnknownKwarg {
            key: "totally-unrelated".into(),
            hint: None,
            allowed: vec!["name".into(), "query".into(), "threshold".into()],
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :totally-unrelated: unknown keyword \
             (allowed: :name, :query, :threshold)"
        );
    }

    #[test]
    fn unknown_kwarg_display_carries_kebab_case_keys_unchanged() {
        // `:notify-ref`, `:window-seconds`, every kebab-cased kwarg name
        // round-trips through both the offending-key slot AND the
        // allowed-list slot unchanged. Pinning this contract means a
        // regression that camelCases or lowercases either side fails-
        // loudly here.
        let err = LispError::UnknownKwarg {
            key: "windou-seconds".into(),
            hint: Some("window-seconds".into()),
            allowed: vec!["notify-ref".into(), "window-seconds".into()],
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :windou-seconds: unknown keyword \
             (did you mean :window-seconds?; allowed: :notify-ref, :window-seconds)"
        );
    }

    #[test]
    fn duplicate_kwarg_display_matches_legacy_compile_shape() {
        // The variant renders byte-for-byte the same string the legacy
        // `Compile { form: ":name", message: "duplicate keyword" }` shape
        // produced, so authoring tools (REPL, LSP, `tatara-check`) that
        // substring-match on the rendered diagnostic see no drift; tools
        // that pattern-match on the variant gain structural binding.
        let err = LispError::DuplicateKwarg { key: "name".into() };
        assert_eq!(
            format!("{err}"),
            "compile error in :name: duplicate keyword"
        );
    }

    #[test]
    fn duplicate_kwarg_display_carries_kebab_case_keys_unchanged() {
        // `:notify-ref`, `:window-seconds`, every kebab-cased kwarg name
        // round-trips through the variant's Display unchanged. Pinning
        // this contract means a regression that camelCases or lowercases
        // the key in the rendered message fails-loudly.
        let err = LispError::DuplicateKwarg {
            key: "notify-ref".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :notify-ref: duplicate keyword"
        );
    }

    #[test]
    fn missing_kwarg_display_matches_legacy_compile_shape() {
        // The variant renders byte-for-byte the same string the legacy
        // `Compile { form: ":threshold", message: "required but not provided" }`
        // shape produced, so authoring tools (REPL, LSP, `tatara-check`) that
        // substring-match on the rendered diagnostic see no drift; tools
        // that pattern-match on the variant gain structural binding.
        let err = LispError::MissingKwarg {
            key: "threshold".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: required but not provided"
        );
    }

    #[test]
    fn missing_kwarg_display_carries_kebab_case_keys_unchanged() {
        // `:notify-ref`, `:window-seconds`, every kebab-cased kwarg name
        // round-trips through the variant's Display unchanged. Pinning
        // this contract means a regression that camelCases or lowercases
        // the key in the rendered message fails-loudly.
        let err = LispError::MissingKwarg {
            key: "notify-ref".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :notify-ref: required but not provided"
        );
    }

    #[test]
    fn unbound_template_var_display_without_hint_matches_legacy_compile_shape() {
        // Without a hint the variant renders byte-for-byte the same string
        // the legacy `Compile { form: ",x", message: "unbound" }` shape
        // produced, so authoring tools that substring-match on the rendered
        // diagnostic see no drift.
        let err = LispError::UnboundTemplateVar {
            prefix: ",",
            name: "y".into(),
            hint: None,
        };
        assert_eq!(format!("{err}"), "compile error in ,y: unbound");
    }

    #[test]
    fn unbound_template_var_display_appends_hint_suffix_when_present() {
        // With a hint the message gains a `"; did you mean ,X?"` suffix —
        // the prefix is preserved in the hint so the operator can copy-paste
        // the suggestion verbatim.
        let err = LispError::UnboundTemplateVar {
            prefix: ",",
            name: "xs".into(),
            hint: Some("x".into()),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,xs: unbound; did you mean ,x?"
        );
    }

    #[test]
    fn unknown_domain_keyword_display_with_hint_renders_did_you_mean_then_registered_list() {
        // The variant names the offending keyword's full call shape
        // (`(defmoniter ...)`), the structural near-miss in the same call
        // shape (`(defmonitor ...)`), and the sorted registered set.
        // Authoring tools that pattern-match on the variant gain
        // structural binding to `keyword` / `hint` / `registered` —
        // tools that substring-match on the rendered diagnostic see a
        // stable shape.
        let err = LispError::UnknownDomainKeyword {
            keyword: "defmoniter".into(),
            hint: Some("defmonitor".into()),
            registered: vec![
                "defalertpolicy".into(),
                "defmonitor".into(),
                "defnotify".into(),
            ],
        };
        assert_eq!(
            format!("{err}"),
            "unknown domain keyword: (defmoniter ...) \
             (did you mean (defmonitor ...)?; \
             registered: defalertpolicy, defmonitor, defnotify)"
        );
    }

    #[test]
    fn unknown_domain_keyword_display_without_hint_renders_registered_list_only() {
        // No near-miss within the bounded edit distance: the rendered
        // message has the registered list but no `did you mean` clause.
        // A wrong hint is worse than no hint — the slot stays empty
        // unless `suggest_keyword` ranks a candidate within the bound.
        let err = LispError::UnknownDomainKeyword {
            keyword: "totally-unrelated".into(),
            hint: None,
            registered: vec!["defalertpolicy".into(), "defmonitor".into()],
        };
        assert_eq!(
            format!("{err}"),
            "unknown domain keyword: (totally-unrelated ...) \
             (registered: defalertpolicy, defmonitor)"
        );
    }

    #[test]
    fn unknown_domain_keyword_display_with_empty_registry_renders_no_domains_registered() {
        // The substrate names the structural reason — the registry has
        // no candidates at all — instead of a misleading empty
        // `registered: ` suffix. A typo against an empty registry is
        // unambiguously a registry-seeding bug, not a near-miss.
        let err = LispError::UnknownDomainKeyword {
            keyword: "defmonitor".into(),
            hint: None,
            registered: vec![],
        };
        assert_eq!(
            format!("{err}"),
            "unknown domain keyword: (defmonitor ...) (no domains registered)"
        );
    }

    #[test]
    fn unknown_domain_keyword_display_carries_kebab_case_keywords_unchanged() {
        // Kebab-cased domain keywords (`defalert-policy`,
        // `defprocess-spec`) round-trip through the offending-keyword
        // slot AND the registered-list slot unchanged. Pinning this
        // contract means a regression that camelCases or lowercases
        // either side fails-loudly here.
        let err = LispError::UnknownDomainKeyword {
            keyword: "defalert-policiy".into(),
            hint: Some("defalert-policy".into()),
            registered: vec!["defalert-policy".into(), "defprocess-spec".into()],
        };
        assert_eq!(
            format!("{err}"),
            "unknown domain keyword: (defalert-policiy ...) \
             (did you mean (defalert-policy ...)?; \
             registered: defalert-policy, defprocess-spec)"
        );
    }

    #[test]
    fn non_symbol_unquote_target_display_renders_canonical_type_mismatch_shape() {
        // `,(list 1 2)` — the inner is a list, not a symbol. The variant
        // names the syntactic marker (`,`), the expected shape (`symbol` —
        // the only form a no-evaluator template can substitute), and the
        // offending literal (`(list 1 2)`) as first-class fields. Authoring
        // tools that pattern-match on the variant gain structural binding;
        // tools that substring-match on the rendered diagnostic see a
        // stable shape parallel to the existing `TypeMismatch` variant.
        let err = LispError::NonSymbolUnquoteTarget {
            prefix: ",",
            got: "(list 1 2)".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,: expected symbol, got (list 1 2)"
        );
    }

    #[test]
    fn non_symbol_unquote_target_display_preserves_splice_prefix() {
        // Splice marker rides through the `prefix` field; the rendered
        // diagnostic is `,@`, not `,`. The operator never has to translate
        // `,` ↔ `,@` mentally — same posture as `UnboundTemplateVar`.
        let err = LispError::NonSymbolUnquoteTarget {
            prefix: ",@",
            got: "5".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,@: expected symbol, got 5"
        );
    }

    #[test]
    fn non_symbol_unquote_target_display_carries_keyword_atom_unchanged() {
        // `,:foo` — the inner is a keyword atom. The `:foo` form
        // round-trips through `Sexp::Display` into the variant's `got`
        // slot unchanged, so the operator sees what they wrote.
        let err = LispError::NonSymbolUnquoteTarget {
            prefix: ",",
            got: ":foo".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,: expected symbol, got :foo"
        );
    }

    #[test]
    fn splice_outside_list_display_renders_legacy_substring_with_offending_form() {
        // `,@xs` at the body's top level — there is no containing list to
        // splice into. The variant names the offending inner (`xs`) as a
        // first-class field so authoring tools (REPL, LSP, `tatara-check`)
        // gain structural binding; tools that substring-match on the
        // rendered diagnostic still see the legacy `"\`,@\` may only appear
        // inside a list"` substring verbatim.
        let err = LispError::SpliceOutsideList { got: "xs".into() };
        assert_eq!(
            format!("{err}"),
            "compile error in ,@: `,@` may only appear inside a list (got ,@xs)"
        );
    }

    #[test]
    fn splice_outside_list_display_carries_list_literal_unchanged() {
        // The offending inner is a list literal — `,@(list 1 2)` — so the
        // operator sees the literal value they wrote in the parenthetical,
        // not just a type-name. Round-trips through `Sexp::Display`.
        let err = LispError::SpliceOutsideList {
            got: "(list 1 2)".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,@: `,@` may only appear inside a list (got ,@(list 1 2))"
        );
    }

    #[test]
    fn splice_outside_list_display_carries_kebab_case_symbol_unchanged() {
        // `,@notify-ref` — kebab-cased symbol round-trips through the
        // variant's `got` slot unchanged. Pinning this contract means a
        // regression that camelCases or lowercases the offending form fails
        // -loudly here.
        let err = LispError::SpliceOutsideList {
            got: "notify-ref".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,@: `,@` may only appear inside a list (got ,@notify-ref)"
        );
    }

    #[test]
    fn splice_outside_list_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substring as a separate assertion so a regression
        // that drifts the wording (e.g., to "outside a list" or "without a
        // containing list") fails-loudly here even if the parenthetical
        // changes shape. The substring is what consumers downstream
        // (tatara-check, the REPL) substring-match on today.
        let err = LispError::SpliceOutsideList { got: "xs".into() };
        let msg = format!("{err}");
        assert!(
            msg.contains("`,@` may only appear inside a list"),
            "expected legacy substring in message, got: {msg}"
        );
        assert!(
            msg.contains("(got ,@xs)"),
            "expected offending-form parenthetical in message, got: {msg}"
        );
    }

    #[test]
    fn unbound_template_var_display_preserves_splice_prefix_in_hint() {
        // Splice marker rides through both the form and the suggestion; the
        // operator never has to translate `,` ↔ `,@` mentally.
        let err = LispError::UnboundTemplateVar {
            prefix: ",@",
            name: "rsts".into(),
            hint: Some("rest".into()),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,@rsts: unbound; did you mean ,@rest?"
        );
    }
}
