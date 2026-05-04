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
}

fn unbound_hint_suffix(prefix: &str, hint: Option<&str>) -> String {
    match hint {
        Some(h) => format!("; did you mean {prefix}{h}?"),
        None => String::new(),
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
            | Self::DuplicateKwarg { .. } => None,
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
